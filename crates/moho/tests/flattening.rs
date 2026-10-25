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
