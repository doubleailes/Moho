#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::data_source::locate;
use moho::data_source::retained::{RetainedContainer, RetainedVector, container, value};
use moho::filter::FlatteningSceneIndex;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::openusd::usd::TimeCode;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Prim, RetainedSceneIndex, SceneIndex};
use moho::schema::{PrimvarsSchema, PurposeSchema, VisibilitySchema, XformSchema};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "a"
{
    double3 xformOp:translate = (1, 0, 0)
    uniform token[] xformOpOrder = ["xformOp:translate"]
    token visibility = "invisible"
    uniform token purpose = "proxy"
    color3f[] primvars:tint = [(1, 0, 0)] (
        interpolation = "constant"
    )
    float[] primvars:perVertex = [1, 2] (
        interpolation = "vertex"
    )

    def Mesh "b"
    {
        double3 xformOp:translate = (0, 2, 0)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "reset"
    {
        double3 xformOp:translate = (0, 0, 7)
        uniform token[] xformOpOrder = ["!resetXformStack!", "xformOp:translate"]
    }
    def Xform "plain"
    {
        def Mesh "deep"
        {
        }
    }
}
def Xform "moving"
{
    double3 xformOp:translate.timeSamples = {
        1: (0, 0, 0),
        3: (2, 0, 0),
    }
    uniform token[] xformOpOrder = ["xformOp:translate"]
    def Mesh "child"
    {
    }
}
"#;

fn setup() -> (common::TempDir, Rc<StageSceneIndex>, Rc<FlatteningSceneIndex>) {
    let (dir, stage) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    stage_index.set_time(TimeCode::new(1.0));
    let flat = FlatteningSceneIndex::new(stage_index.clone());
    (dir, stage_index, flat)
}

fn world(flat: &FlatteningSceneIndex, path: &str) -> gf::Mat4d {
    XformSchema::get(&flat.prim(&p(path)).data.unwrap())
        .unwrap()
        .matrix_value()
        .unwrap()
}

#[test]
fn nested_transforms_and_reset() {
    let (_dir, _stage, flat) = setup();
    assert_eq!(world(&flat, "/a/b"), gf::Mat4d::translation([1.0, 2.0, 0.0]));
    assert_eq!(world(&flat, "/a/reset"), gf::Mat4d::translation([0.0, 0.0, 7.0]));
    assert_eq!(
        world(&flat, "/a/plain/deep"),
        gf::Mat4d::translation([1.0, 0.0, 0.0]),
        "no transform: the parent's"
    );
    let b = XformSchema::get(&flat.prim(&p("/a/b")).data.unwrap()).unwrap();
    assert_eq!(b.reset_xform_stack(), Some(true));
}

#[test]
fn a_moving_parent_reports_its_samples() {
    let (_dir, stage_index, flat) = setup();
    stage_index.set_time(TimeCode::new(2.0));
    let child = XformSchema::get(&flat.prim(&p("/moving/child")).data.unwrap()).unwrap();
    assert_eq!(
        child.matrix().unwrap().sample_times(-0.25, 0.25),
        Some(vec![-1.0, 1.0])
    );
    assert_eq!(
        child.matrix_value(),
        Some(gf::Mat4d::translation([1.0, 0.0, 0.0]))
    );
}

#[test]
fn invisible_subtree_and_inherited_purpose() {
    let (_dir, _stage, flat) = setup();
    let vis = |path: &str| {
        VisibilitySchema::get(&flat.prim(&p(path)).data.unwrap())
            .unwrap()
            .visibility()
    };
    assert_eq!(vis("/a"), Some(false));
    assert_eq!(vis("/a/b"), Some(false));
    assert_eq!(vis("/a/plain/deep"), Some(false));
    assert_eq!(vis("/moving/child"), Some(true));
    let purpose = PurposeSchema::get(&flat.prim(&p("/a/b")).data.unwrap()).unwrap();
    assert_eq!(purpose.purpose(), Some(Token::from("proxy")));
}

#[test]
fn constant_primvars_are_inherited() {
    let (_dir, _stage, flat) = setup();
    let primvars = PrimvarsSchema::get(&flat.prim(&p("/a/b")).data.unwrap()).unwrap();
    let names: Vec<String> = primvars.names().iter().map(ToString::to_string).collect();
    assert!(names.contains(&"tint".to_owned()), "{names:?}");
    assert!(
        !names.contains(&"perVertex".to_owned()),
        "only constant primvars are inherited"
    );
    assert_eq!(
        primvars.primvar("tint").unwrap().flattened_value(0.0),
        Some(Value::Vec3fVec(vec![gf::Vec3f {
            x: 1.0,
            y: 0.0,
            z: 0.0
        }]))
    );
}

#[test]
fn a_parent_move_dirties_its_subtree() {
    let (_dir, stage_index, flat) = setup();
    for path in ["/a/b", "/a/plain/deep", "/a/reset"] {
        world(&flat, path);
    }
    let recorder = RecordingObserver::on(flat.as_ref());
    stage_index
        .stage()
        .unwrap()
        .attribute("/a.xformOp:translate")
        .unwrap()
        .set(gf::Vec3d {
            x: 5.0,
            y: 0.0,
            z: 0.0,
        })
        .unwrap();
    stage_index.apply_pending_updates();
    let mut dirtied: Vec<String> = recorder
        .take()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) if d.locators.intersects(&Locator::parse("xform")) => Some(d.path.to_string()),
            _ => None,
        })
        .collect();
    dirtied.sort();
    assert_eq!(dirtied, ["/a", "/a/b", "/a/plain", "/a/plain/deep", "/a/reset"]);
    assert_eq!(world(&flat, "/a/b"), gf::Mat4d::translation([5.0, 2.0, 0.0]));
}

#[test]
fn only_what_was_read_is_dirtied_below() {
    // `HdFlatteningSceneIndex` dirties the descendants whose flattened data
    // it computed: none before any read, and once dirtied, none until read
    // again.
    let (_dir, stage_index, flat) = setup();
    let recorder = RecordingObserver::on(flat.as_ref());
    let move_a = |x: f64| {
        stage_index
            .stage()
            .unwrap()
            .attribute("/a.xformOp:translate")
            .unwrap()
            .set(gf::Vec3d { x, y: 0.0, z: 0.0 })
            .unwrap();
        stage_index.apply_pending_updates();
        let mut dirtied: Vec<String> = recorder
            .take()
            .into_iter()
            .filter_map(|n| match n {
                Notice::Dirtied(d) => Some(d.path.to_string()),
                _ => None,
            })
            .collect();
        dirtied.sort();
        dirtied
    };
    assert_eq!(move_a(5.0), ["/a"], "nothing read");
    world(&flat, "/a/plain/deep");
    assert_eq!(move_a(6.0), ["/a", "/a/plain", "/a/plain/deep"], "the path read");
    assert_eq!(move_a(7.0), ["/a"], "dirtied, then not read again");
    // A visibility read is not an xform read.
    VisibilitySchema::get(&flat.prim(&p("/a/b")).data.unwrap())
        .unwrap()
        .visibility();
    assert_eq!(move_a(8.0), ["/a"]);
}

#[test]
fn purpose_is_flattened_as_hydra() {
    // `HdFlattenedPurposeDataSourceProvider`: an own purpose passes the
    // input container through, an inheritable parent's is taken whole, and
    // a fallback alone gives `purpose` and nothing else.
    let input = RetainedSceneIndex::new();
    let purpose = |entries: Vec<(&str, moho::data_source::DataSource)>| {
        RetainedContainer::new([("purpose", RetainedContainer::new(entries).into_data_source())])
            .into_handle()
    };
    let empty: [(&str, moho::data_source::DataSource); 0] = [];
    input.add_prims([
        (
            p("/a"),
            Prim::new(
                "",
                purpose(vec![
                    ("purpose", value(Value::Token(Token::from("proxy")))),
                    ("inheritable", value(true)),
                ]),
            ),
        ),
        (
            p("/a/b"),
            Prim::new("mesh", RetainedContainer::new(empty).into_handle()),
        ),
        (
            p("/c"),
            Prim::new(
                "mesh",
                purpose(vec![("fallback", value(Value::Token(Token::from("guide"))))]),
            ),
        ),
    ]);
    let flat = FlatteningSceneIndex::new(input.clone());
    let names = |path: &str| {
        let data = flat.prim(&p(path)).data.unwrap();
        locate(&data, &Locator::parse("purpose"))
            .unwrap()
            .as_container()
            .unwrap()
            .names()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names("/a"),
        ["purpose", "inheritable"],
        "the input's own container"
    );
    assert_eq!(
        names("/a/b"),
        ["purpose", "inheritable"],
        "the parent's, inherited"
    );
    assert_eq!(names("/c"), ["purpose"], "a fallback is not inheritable");
    assert_eq!(
        read(&flat, "/c", "purpose/purpose"),
        Some(Value::Token(Token::from("guide")))
    );
    assert_eq!(read(&flat, "/c", "purpose/inheritable"), None);
}

#[test]
fn material_bindings_concatenate_down() {
    let input = RetainedSceneIndex::new();
    let bindings = |path: &str| {
        container([(
            "",
            RetainedVector::new([value(Value::PathVec(vec![p(path)]))]).into_data_source(),
        )])
    };
    input.add_prims([
        (
            p("/a"),
            Prim::new(
                "",
                RetainedContainer::new([("usdMaterialBindings", bindings("/Looks/a"))]).into_handle(),
            ),
        ),
        (
            p("/a/b"),
            Prim::new(
                "mesh",
                RetainedContainer::new([("usdMaterialBindings", bindings("/Looks/b"))]).into_handle(),
            ),
        ),
    ]);
    let flat = FlatteningSceneIndex::new(input.clone());
    let data = flat.prim(&p("/a/b")).data.unwrap();
    let all = locate(&data, &Locator::new(["usdMaterialBindings", ""])).unwrap();
    let all = all.as_vector().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(
        all.get(0).unwrap().value(),
        Some(Value::PathVec(vec![p("/Looks/a")])),
        "ancestors first"
    );
    let recorder = RecordingObserver::on(flat.as_ref());
    input.dirty_prims([(p("/a"), LocatorSet::from(Locator::parse("usdMaterialBindings")))]);
    assert_eq!(recorder.take().len(), 2, "/a and /a/b");
}

const MODELS: &str = r#"#usda 1.0
(
    upAxis = "Z"
)
def Xform "asset" (
    kind = "component"
    assetInfo = {
        string name = "asset"
    }
    prepend apiSchemas = ["GeomModelAPI"]
)
{
    uniform token model:drawMode = "bounds"
    uniform token model:cardGeometry = "cross"

    def Xform "part" (
        prepend apiSchemas = ["GeomModelAPI"]
    )
    {
        uniform token model:cardGeometry = "box"
        def Mesh "geo"
        {
        }
    }
}
"#;

fn read(flat: &FlatteningSceneIndex, path: &str, locator: &str) -> Option<Value> {
    locate(&flat.prim(&p(path)).data?, &Locator::parse(locator)).and_then(|d| d.value())
}

#[test]
fn the_up_axis_reaches_every_prim() {
    let (_dir, stage) = stage(MODELS);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    let flat = FlatteningSceneIndex::new(stage_index);
    for path in ["/asset", "/asset/part", "/asset/part/geo"] {
        assert_eq!(
            read(&flat, path, "__usdUpAxis/upAxis"),
            Some(Value::Token(Token::from("Z"))),
            "{path}"
        );
    }
}

#[test]
fn model_data_is_inherited() {
    let (_dir, stage) = stage(MODELS);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage);
    let flat = FlatteningSceneIndex::new(stage_index);
    assert_eq!(
        read(&flat, "/asset/part/geo", "model/modelPath"),
        read(&flat, "/asset", "model/modelPath"),
        "the model's own data reaches its descendants"
    );
    assert!(read(&flat, "/asset", "model/modelPath").is_some());
    // A prim with its own geomModel inherits the draw mode it lacks.
    assert_eq!(
        read(&flat, "/asset/part", "geomModel/drawMode"),
        Some(Value::Token(Token::from("bounds")))
    );
    assert_eq!(
        read(&flat, "/asset/part", "geomModel/cardGeometry"),
        Some(Value::Token(Token::from("box"))),
        "its own data stays"
    );
    // A prim without one takes its parent's whole.
    assert_eq!(
        read(&flat, "/asset/part/geo", "geomModel/cardGeometry"),
        Some(Value::Token(Token::from("box")))
    );
}

#[test]
fn descendants_are_dirtied_for_what_they_inherit() {
    let input = RetainedSceneIndex::new();
    let empty = || Prim::new("", RetainedContainer::default().into_handle());
    input.add_prims([(p("/a"), empty()), (p("/a/b"), empty())]);
    let flat = FlatteningSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(flat.as_ref());
    let child_locators = |locators: &[&str]| -> Vec<String> {
        // Read every flattened name of the child first: only data read is
        // dirtied below.
        let data = flat.prim(&p("/a/b")).data.unwrap();
        for name in [
            "xform",
            "visibility",
            "purpose",
            "usdMaterialBindings",
            "geomModel",
            "model",
            "__usdUpAxis",
        ] {
            data.get(&Token::from(name));
        }
        locate(&data, &Locator::parse("primvars/tint"));
        input.dirty_prims([(
            p("/a"),
            locators.iter().map(|l| Locator::parse(l)).collect::<LocatorSet>(),
        )]);
        recorder
            .take()
            .into_iter()
            .filter_map(|n| match n {
                Notice::Dirtied(d) if d.path == p("/a/b") => Some(d.locators),
                _ => None,
            })
            .flat_map(|set| set.iter().map(ToString::to_string).collect::<Vec<_>>())
            .collect()
    };
    assert_eq!(child_locators(&["xform/matrix"]), ["xform"]);
    assert_eq!(child_locators(&["visibility/visibility"]), ["visibility"]);
    assert_eq!(
        child_locators(&["primvars/tint/primvarValue"]),
        ["primvars/tint/primvarValue"]
    );
    assert_eq!(child_locators(&["primvars/tint/interpolation"]), ["primvars"]);
    assert_eq!(
        child_locators(&["usdMaterialBindings/preview/0"]),
        ["usdMaterialBindings/preview"]
    );
    assert_eq!(child_locators(&["model/assetName"]), ["model/assetName"]);
    assert_eq!(child_locators(&["geomModel/drawMode"]), ["geomModel/drawMode"]);
    assert!(child_locators(&["geomModel/cardGeometry"]).is_empty());
    assert_eq!(child_locators(&["geomModel"]), ["geomModel"]);
    assert!(child_locators(&["mesh/topology"]).is_empty());
}

#[test]
fn a_skipped_subtree_stays_skipped_beside_a_sibling_that_extends_its_name() {
    // `_DirtyHierarchy` skips the subtree of a prim that did not read the
    // dirtied data. Paths are walked in text order, where `/w/ax` shares
    // `/w/a`'s text prefix without being under it, and a wide group of
    // skipped children costs one ancestor walk per prim.
    let input = RetainedSceneIndex::new();
    let prim = || {
        let empty: [(&str, moho::data_source::DataSource); 0] = [];
        Prim::new("mesh", RetainedContainer::new(empty).into_handle())
    };
    let mut prims = vec![
        (p("/w"), prim()),
        (p("/w/a"), prim()),
        (p("/w/a/b"), prim()),
        (p("/w/ax"), prim()),
        (p("/w/ax/c"), prim()),
    ];
    let wide: Vec<String> = (0..2000).map(|i| format!("/w/g/m{i}")).collect();
    prims.push((p("/w/g"), prim()));
    prims.extend(wide.iter().map(|path| (p(path), prim())));
    input.add_prims(prims);
    let flat = FlatteningSceneIndex::new(input.clone());
    let visibility = |path: &str| {
        VisibilitySchema::get(&flat.prim(&p(path)).data.unwrap()).map(|v| v.visibility());
    };
    // `/w/a`, `/w/a/b` and the wide group read visibility only; `/w/ax/c`
    // reads its world transform, through `/w/ax` and `/w`.
    for path in ["/w/a", "/w/a/b", "/w/g"]
        .iter()
        .copied()
        .chain(wide.iter().map(String::as_str))
    {
        visibility(path);
    }
    world(&flat, "/w/ax/c");
    let recorder = RecordingObserver::on(flat.as_ref());
    input.dirty_prims([(p("/w"), LocatorSet::from(Locator::parse("xform")))]);
    let mut dirtied: Vec<String> = recorder
        .take()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Dirtied(d) => Some(d.path.to_string()),
            _ => None,
        })
        .collect();
    dirtied.sort();
    assert_eq!(dirtied, ["/w", "/w/ax", "/w/ax/c"]);
}
