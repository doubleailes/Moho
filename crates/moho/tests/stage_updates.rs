#![allow(clippy::unwrap_used)]

mod common;

use common::{p, stage};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, Removed, SceneIndex};
use moho::stage::StageSceneIndex;

const SCENE: &str = r#"#usda 1.0
def Xform "world"
{
    def Mesh "geo"
    {
        int myRenderer:rayMask = 6
        int myRenderer:other = 1
    }
    def Camera "cam"
    {
        int myRenderer:rayMask = 2
    }
    def Mesh "off" (
        active = false
    )
    {
        int myRenderer:rayMask = 2
    }
}
"#;

fn setup() -> (
    common::TempDir,
    std::rc::Rc<StageSceneIndex>,
    std::rc::Rc<RecordingObserver>,
) {
    let (dir, stage) = stage(SCENE);
    let index = StageSceneIndex::new();
    index.set_stage(stage);
    let recorder = RecordingObserver::on(index.as_ref());
    (dir, index, recorder)
}

fn set_int(index: &StageSceneIndex, attr: &str, value: i32) {
    index
        .stage()
        .unwrap()
        .attribute(attr)
        .unwrap()
        .set(value)
        .unwrap();
}

fn attrs(names: &[&str]) -> LocatorSet {
    names
        .iter()
        .map(|n| Locator::new(["__usdAttributes", n]))
        .collect()
}

#[test]
fn nothing_until_applied() {
    let (_dir, index, recorder) = setup();
    set_int(&index, "/world/geo.myRenderer:rayMask", 7);
    assert!(recorder.take().is_empty());
    index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [Notice::Dirtied(Dirtied {
            path: p("/world/geo"),
            locators: attrs(&["myRenderer:rayMask"])
        })]
    );
    index.apply_pending_updates();
    assert!(recorder.take().is_empty(), "applying twice notifies once");
}

#[test]
fn a_prim_defined_and_another_removed() {
    let (_dir, index, recorder) = setup();
    let stage = index.stage().unwrap();
    stage.define_typed_prim("/world/new", "Sphere").unwrap();
    stage.remove_prim("/world/cam").unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Removed(Removed {
            path: p("/world/cam")
        })),
        "{notices:?}"
    );
    assert!(
        notices.contains(&Notice::Added(Added {
            path: p("/world/new"),
            ty: Token::from("sphere")
        })),
        "{notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|n| matches!(n, Notice::Removed(r) if r.path == p("/world/new"))),
        "a new prim is added, not removed and re-added: {notices:?}"
    );
    assert!(!index.prim(&p("/world/cam")).is_defined());
    assert_eq!(index.prim(&p("/world/new")).ty, "sphere");
}

#[test]
fn a_retyped_prim_is_resynced() {
    // USD notifies a `typeName` edit as a resync, and UsdImaging re-adds the
    // prim as its new type (checked against USD 26.08).
    let (_dir, index, recorder) = setup();
    index
        .stage()
        .unwrap()
        .define_typed_prim("/world/geo", "Sphere")
        .unwrap();
    index.apply_pending_updates();
    assert_eq!(
        recorder.take(),
        [Notice::Added(Added {
            path: p("/world/geo"),
            ty: Token::from("sphere")
        })]
    );
    assert_eq!(index.prim(&p("/world/geo")).ty, "sphere");
}

#[test]
fn several_edits_one_application() {
    let (_dir, index, recorder) = setup();
    set_int(&index, "/world/geo.myRenderer:rayMask", 7);
    set_int(&index, "/world/geo.myRenderer:other", 2);
    set_int(&index, "/world/cam.myRenderer:rayMask", 3);
    index.apply_pending_updates();
    let mut notices = recorder.take();
    notices.sort_by_key(|n| format!("{n:?}"));
    assert_eq!(
        notices,
        [
            Notice::Dirtied(Dirtied {
                path: p("/world/cam"),
                locators: attrs(&["myRenderer:rayMask"])
            }),
            Notice::Dirtied(Dirtied {
                path: p("/world/geo"),
                locators: attrs(&["myRenderer:other", "myRenderer:rayMask"])
            }),
        ]
    );
}

#[test]
fn edits_to_unexposed_prims_are_silent() {
    let (_dir, index, recorder) = setup();
    set_int(&index, "/world/off.myRenderer:rayMask", 9);
    index.apply_pending_updates();
    assert!(recorder.take().is_empty());
}

#[test]
fn activating_a_prim_adds_it() {
    let (_dir, index, recorder) = setup();
    let stage = index.stage().unwrap();
    stage.prim("/world/off").unwrap().set_active(true).unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Added(Added {
            path: p("/world/off"),
            ty: Token::from("mesh")
        })),
        "{notices:?}"
    );
    assert!(index.prim(&p("/world/off")).is_defined());
}

#[test]
fn a_resync_re_adds_without_removing() {
    let (_dir, usd) = stage(
        r#"#usda 1.0
def Xform "src"
{
    def Mesh "extra"
    {
    }
}
def Xform "world"
{
    def Xform "asset"
    {
        def Mesh "geo"
        {
        }
    }
}
"#,
    );
    let index = StageSceneIndex::new();
    index.set_stage(usd);
    let recorder = RecordingObserver::on(index.as_ref());
    let stage = index.stage().unwrap();
    stage
        .prim("/world/asset")
        .unwrap()
        .references()
        .add(moho::openusd::sdf::Reference {
            prim_path: p("/src"),
            ..Default::default()
        })
        .unwrap();
    index.apply_pending_updates();
    let notices = recorder.take();
    assert!(
        !notices.iter().any(|n| matches!(n, Notice::Removed(_))),
        "{notices:?}"
    );
    for (path, ty) in [
        ("/world/asset", ""),
        ("/world/asset/geo", "mesh"),
        ("/world/asset/extra", "mesh"),
    ] {
        assert!(
            notices.contains(&Notice::Added(Added {
                path: p(path),
                ty: Token::from(ty)
            })),
            "{path}: {notices:?}"
        );
    }
}

#[test]
fn a_resync_removes_only_what_goes() {
    let (_dir, index, recorder) = setup();
    let stage = index.stage().unwrap();
    // Deactivating the parent resyncs it: each held prim under it goes.
    stage.prim("/world").unwrap().set_active(false).unwrap();
    index.apply_pending_updates();
    let mut removed: Vec<_> = recorder
        .take()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Removed(r) => Some(r.path),
            _ => None,
        })
        .collect();
    removed.sort();
    assert_eq!(removed, [p("/world"), p("/world/cam"), p("/world/geo")]);
}
