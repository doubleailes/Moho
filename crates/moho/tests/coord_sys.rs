#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::{p, stage};
use moho::data_source::locate;
use moho::data_source::retained::{RetainedContainer, container, value};
use moho::filter::{CoordSysPrimSceneIndex, DependencyForwardingSceneIndex, FlatteningSceneIndex};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf;
use moho::openusd::vt::Value;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, Prim, Removed, RetainedSceneIndex, SceneIndex};
use moho::schema::{CoordSysBindingSchema, CoordSysSchema, XformSchema};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "World" (
    prepend apiSchemas = ["CoordSysAPI:worldSpace", "CoordSysAPI:unbound"]
)
{
    rel coordSys:worldSpace:binding = </World/Space>

    def Xform "Space"
    {
        double3 xformOp:translate = (1, 2, 3)
        uniform token[] xformOpOrder = ["xformOp:translate"]
    }
    def Xform "Other"
    {
    }
    def Mesh "model" (
        prepend apiSchemas = ["CoordSysAPI:worldSpace", "CoordSysAPI:modelSpace"]
    )
    {
        rel coordSys:worldSpace:binding = </World/Other>
        rel coordSys:modelSpace:binding = </World/model>

        def Mesh "leaf"
        {
        }
    }
    def Mesh "legacy"
    {
        rel coordSys:legacySpace = </World/Space>
    }
}
"#;

fn bindings(index: &dyn SceneIndex, path: &str) -> Vec<(String, String)> {
    let Some(data) = index.prim(&p(path)).data else {
        return Vec::new();
    };
    let Some(schema) = CoordSysBindingSchema::get(&data) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = schema
        .bindings()
        .into_iter()
        .map(|(name, path)| (name.as_str().to_owned(), path.as_str().to_owned()))
        .collect();
    out.sort();
    out
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

fn names(index: &dyn SceneIndex, path: &str) -> Vec<String> {
    index
        .prim(&p(path))
        .data
        .map(|d| d.names().iter().map(|n| n.as_str().to_owned()).collect())
        .unwrap_or_default()
}

#[test]
fn a_prim_applying_coord_sys_api_binds_its_bound_instances() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    // An instance targeting nothing binds nothing.
    assert_eq!(
        bindings(index.as_ref(), "/World"),
        pairs(&[("worldSpace", "/World/Space")])
    );
    assert_eq!(
        bindings(index.as_ref(), "/World/model"),
        pairs(&[("modelSpace", "/World/model"), ("worldSpace", "/World/Other")])
    );
    // No binding of its own, and the non-applied form is not read.
    for path in ["/World/Space", "/World/model/leaf", "/World/legacy"] {
        assert!(
            !names(index.as_ref(), path).iter().any(|n| n == "coordSysBinding"),
            "{path}"
        );
    }
}

#[test]
fn a_binding_edit_dirties_coord_sys_binding() {
    let (_dir, usd) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let recorder = RecordingObserver::on(index.as_ref());
    index
        .stage()
        .unwrap()
        .relationship("/World.coordSys:worldSpace:binding")
        .unwrap()
        .set_targets([p("/World/Other")])
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    // The whole `coordSysBinding`, beside the generic relationship data.
    assert!(
        notices.iter().any(|n| matches!(n, Notice::Dirtied(d)
            if d.path == p("/World") && d.locators.contains(&Locator::parse("coordSysBinding")))),
        "{notices:?}"
    );
    assert_eq!(
        bindings(index.as_ref(), "/World"),
        pairs(&[("worldSpace", "/World/Other")])
    );
}

#[test]
fn flattening_overlays_a_prims_bindings_on_its_ancestors() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = FlatteningSceneIndex::new(stage_index);
    assert_eq!(
        bindings(flat.as_ref(), "/World/Space"),
        pairs(&[("worldSpace", "/World/Space")])
    );
    let model = pairs(&[("modelSpace", "/World/model"), ("worldSpace", "/World/Other")]);
    assert_eq!(bindings(flat.as_ref(), "/World/model"), model);
    assert_eq!(bindings(flat.as_ref(), "/World/model/leaf"), model);
    // Every flattened name is named, bound or not.
    assert!(names(flat.as_ref(), "/").iter().any(|n| n == "coordSysBinding"));
    assert!(bindings(flat.as_ref(), "/").is_empty());
}

#[test]
fn a_flattened_binding_edit_dirties_the_descendants_that_read_it() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = FlatteningSceneIndex::new(stage_index.clone());
    bindings(flat.as_ref(), "/World/Space");
    let recorder = RecordingObserver::on(flat.as_ref());
    stage_index
        .stage()
        .unwrap()
        .relationship("/World.coordSys:worldSpace:binding")
        .unwrap()
        .set_targets([p("/World/Other")])
        .unwrap();
    stage_index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Dirtied(Dirtied {
            path: p("/World/Space"),
            locators: LocatorSet::from(Locator::parse("coordSysBinding")),
        })),
        "{notices:?}"
    );
    assert_eq!(
        bindings(flat.as_ref(), "/World/Space"),
        pairs(&[("worldSpace", "/World/Other")])
    );
}

#[test]
fn coord_sys_prims_are_added_under_the_targets() {
    let (_dir, usd) = stage(SCENE);
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(usd);
    let flat = FlatteningSceneIndex::new(stage_index);
    let index = CoordSysPrimSceneIndex::new(flat);

    let space = p("/World/Space/__coordSys_worldSpace");
    assert!(index.children(&p("/World/Space")).contains(&space));
    for path in [
        "/World/Other/__coordSys_worldSpace",
        "/World/model/__coordSys_modelSpace",
    ] {
        assert!(
            index.children(&p(path).parent().unwrap()).contains(&p(path)),
            "{path}"
        );
    }

    let prim = index.prim(&space);
    assert_eq!(prim.ty, "coordSys");
    let data = prim.data.unwrap();
    assert_eq!(
        CoordSysSchema::get(&data).unwrap().name().unwrap().as_str(),
        "worldSpace"
    );
    let matrix = XformSchema::get(&data).unwrap().matrix_value().unwrap();
    assert_eq!(matrix, translation(1.0, 2.0, 3.0));
    assert_eq!(
        locate(
            &data,
            &Locator::parse("__dependencies/xformDependency/dependedOnPrimPath")
        )
        .and_then(|d| d.value()),
        Some(Value::Path(p("/World/Space")))
    );

    // Bindings name the coordSys prims.
    assert_eq!(
        bindings(index.as_ref(), "/World/model/leaf"),
        pairs(&[
            ("modelSpace", "/World/model/__coordSys_modelSpace"),
            ("worldSpace", "/World/Other/__coordSys_worldSpace"),
        ])
    );
    // A prim the filter does not add is not answered.
    assert!(
        index
            .prim(&p("/World/Space/__coordSys_modelSpace"))
            .data
            .is_none()
    );
}

fn binder(bindings: &[(&str, sdf::Path)]) -> Prim {
    let bindings: Vec<(&str, _)> = bindings
        .iter()
        .map(|(name, path)| (*name, value(Value::Path(path.clone()))))
        .collect();
    Prim::new(
        "mesh",
        RetainedContainer::new([("coordSysBinding", container(bindings))]).into_handle(),
    )
}

fn xformable(x: f64) -> Prim {
    Prim::new(
        "",
        RetainedContainer::new([(
            "xform",
            container([("matrix", value(Value::Matrix4d(translation(x, 0.0, 0.0))))]),
        )])
        .into_handle(),
    )
}

fn added(path: &str, ty: &str) -> Notice {
    Notice::Added(Added {
        path: p(path),
        ty: ty.into(),
    })
}

fn removed(path: &str) -> Notice {
    Notice::Removed(Removed { path: p(path) })
}

fn dirtied(path: &str, locator: &str) -> Notice {
    Notice::Dirtied(Dirtied {
        path: p(path),
        locators: LocatorSet::from(Locator::parse(locator)),
    })
}

fn retained() -> (
    Rc<RetainedSceneIndex>,
    Rc<CoordSysPrimSceneIndex>,
    Rc<RecordingObserver>,
) {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/q"), xformable(1.0)), (p("/r"), xformable(2.0))]);
    let index = CoordSysPrimSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    (input, index, recorder)
}

#[test]
fn bindings_added_changed_and_removed_add_and_remove_coord_sys_prims() {
    let (input, index, recorder) = retained();
    input.add_prims([(p("/a"), binder(&[("foo", p("/q"))]))]);
    assert_eq!(
        recorder.take(),
        [added("/a", "mesh"), added("/q/__coordSys_foo", "coordSys")]
    );

    // A second binding of the name to the same prim shares it.
    input.add_prims([(p("/b"), binder(&[("foo", p("/q"))]))]);
    assert_eq!(recorder.take(), [added("/b", "mesh")]);

    // Retargeted: the new target's prim added, the old one kept for /b.
    input.add_prims([(p("/a"), binder(&[("foo", p("/r"))]))]);
    assert_eq!(
        recorder.take(),
        [added("/a", "mesh"), added("/r/__coordSys_foo", "coordSys")]
    );

    input.remove_prims([p("/b")]);
    assert_eq!(recorder.take(), [removed("/b"), removed("/q/__coordSys_foo")]);
    assert!(index.prim(&p("/q/__coordSys_foo")).data.is_none());
    assert!(!index.children(&p("/q")).contains(&p("/q/__coordSys_foo")));
    assert!(index.prim(&p("/r/__coordSys_foo")).data.is_some());
}

#[test]
fn removing_an_ancestor_removes_its_descendants_bindings() {
    let (input, index, recorder) = retained();
    input.add_prims([
        (p("/g"), Prim::new("", RetainedContainer::default().into_handle())),
        (p("/g/a"), binder(&[("foo", p("/q")), ("bar", p("/r"))])),
        // A sibling whose name extends the removed prim's is kept.
        (p("/g2"), binder(&[("baz", p("/q"))])),
    ]);
    recorder.take();
    input.remove_prims([p("/g")]);
    assert_eq!(
        recorder.take(),
        [
            removed("/g"),
            removed("/q/__coordSys_foo"),
            removed("/r/__coordSys_bar")
        ]
    );
    assert!(index.prim(&p("/q/__coordSys_baz")).data.is_some());
}

#[test]
fn a_dirtied_binding_is_counted_again_quirk_included() {
    let (input, index, recorder) = retained();
    input.add_prims([(p("/a"), binder(&[("foo", p("/q"))]))]);
    recorder.take();
    // Hydra drops the prim's bindings and counts them again: the only
    // binding of its prim notifies it added, then removed.
    input.dirty_prims([(p("/a"), LocatorSet::from(Locator::parse("coordSysBinding")))]);
    assert_eq!(
        recorder.take(),
        [
            dirtied("/a", "coordSysBinding"),
            added("/q/__coordSys_foo", "coordSys"),
            removed("/q/__coordSys_foo")
        ]
    );
    // The index still answers it.
    assert!(index.prim(&p("/q/__coordSys_foo")).data.is_some());
    // Another locator recounts nothing.
    input.dirty_prims([(p("/a"), LocatorSet::from(Locator::parse("xform")))]);
    assert_eq!(recorder.take(), [dirtied("/a", "xform")]);
}

#[test]
fn a_binding_to_a_non_prim_path_is_left_alone() {
    let (input, index, recorder) = retained();
    input.add_prims([(p("/a"), binder(&[("foo", p("/q.attr"))]))]);
    assert_eq!(recorder.take(), [added("/a", "mesh")]);
    assert_eq!(bindings(index.as_ref(), "/a"), pairs(&[("foo", "/q.attr")]));
}

#[test]
fn a_target_transform_edit_reaches_the_coord_sys_prim_through_dependencies() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/q"), xformable(1.0)), (p("/a"), binder(&[("foo", p("/q"))]))]);
    let coord_sys = CoordSysPrimSceneIndex::new(input.clone());
    let index = DependencyForwardingSceneIndex::new(coord_sys);
    // Dependencies count once their declaring prim has been read.
    index.prim(&p("/q/__coordSys_foo"));
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([(p("/q"), LocatorSet::from(Locator::parse("xform")))]);
    let notices = recorder.take();
    assert!(
        notices.contains(&dirtied("/q/__coordSys_foo", "xform")),
        "{notices:?}"
    );
}

fn translation(x: f64, y: f64, z: f64) -> gf::Mat4d {
    gf::Mat4d::translation(gf::Vec3d { x, y, z })
}
