#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::p;
use moho::data_source::locate;
use moho::data_source::retained::{RetainedContainer, value};
use moho::filter::MergingSceneIndex;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Prim, Removed, RetainedSceneIndex, SceneIndex, SceneIndexHandle};

fn prim(ty: &str, entries: &[(&str, i32)]) -> Prim {
    Prim::new(
        ty,
        RetainedContainer::new(entries.iter().map(|(n, v)| (*n, value(*v)))).into_handle(),
    )
}

fn read(index: &dyn SceneIndex, path: &str, name: &str) -> Option<Value> {
    locate(&index.prim(&p(path)).data?, &Locator::parse(name)).and_then(|d| d.value())
}

/// A scene input holding the hierarchy, and a chunk holding one element's
/// payload contents, as S0's streaming does.
fn scene_and_chunk() -> (Rc<RetainedSceneIndex>, Rc<RetainedSceneIndex>) {
    let scene = RetainedSceneIndex::new();
    scene.add_prims([
        (p("/world"), prim("", &[("a", 1)])),
        (p("/world/elem"), prim("", &[("fromScene", 1), ("shared", 1)])),
        (p("/world/other"), prim("", &[])),
    ]);
    let chunk = RetainedSceneIndex::new();
    chunk.add_prims([
        (p("/world"), prim("", &[("ignored", 1)])),
        (p("/world/elem"), prim("", &[("fromChunk", 2), ("shared", 2)])),
        (p("/world/elem/geo"), prim("mesh", &[])),
    ]);
    (scene, chunk)
}

#[test]
fn two_inputs_share_a_prim() {
    let (scene, chunk) = scene_and_chunk();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(chunk.clone(), p("/world/elem"));
    merged.add_input_scene(scene.clone(), sdf_root());
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "shared"),
        Some(Value::Int(2)),
        "the chunk is stronger"
    );
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "fromScene"),
        Some(Value::Int(1))
    );
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "fromChunk"),
        Some(Value::Int(2))
    );
    assert_eq!(
        read(merged.as_ref(), "/world", "ignored"),
        None,
        "an input answers only under its root"
    );
    assert_eq!(merged.prim(&p("/world/elem/geo")).ty, "mesh");
    assert_eq!(
        merged.children(&p("/world")),
        [p("/world/elem"), p("/world/other")]
    );
}

#[test]
fn a_chunk_comes_and_goes() {
    let (scene, chunk) = scene_and_chunk();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(scene.clone(), sdf_root());
    let recorder = RecordingObserver::on(merged.as_ref());
    let chunk_handle: SceneIndexHandle = chunk.clone();
    merged.insert_input_scene(0, chunk_handle.clone(), p("/world/elem"));
    let added = recorder.take();
    assert!(
        added.contains(&Notice::Added(Added {
            path: p("/world/elem/geo"),
            ty: Token::from("mesh")
        })),
        "{added:?}"
    );
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "shared"),
        Some(Value::Int(2))
    );

    merged.remove_input_scene(&chunk_handle);
    let notices = recorder.take();
    assert!(
        notices.contains(&Notice::Removed(Removed {
            path: p("/world/elem/geo")
        })),
        "{notices:?}"
    );
    assert!(
        notices.contains(&Notice::Added(Added {
            path: p("/world/elem"),
            ty: Token::default()
        })),
        "{notices:?}"
    );
    assert!(!merged.prim(&p("/world/elem/geo")).is_defined());
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "shared"),
        Some(Value::Int(1)),
        "back to the scene input"
    );

    chunk.dirty_prims([(p("/world/elem/geo"), Default::default())]);
    assert!(
        recorder.take().is_empty(),
        "a removed input's notices are ignored"
    );
}

#[test]
fn a_weaker_input_removes_a_shared_prim() {
    let (scene, chunk) = scene_and_chunk();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(chunk.clone(), p("/world/elem"));
    merged.add_input_scene(scene.clone(), sdf_root());
    let recorder = RecordingObserver::on(merged.as_ref());
    scene.remove_prims([p("/world/elem")]);
    let notices = recorder.take();
    assert_eq!(
        notices.first(),
        Some(&Notice::Removed(Removed {
            path: p("/world/elem")
        }))
    );
    assert!(
        notices.contains(&Notice::Added(Added {
            path: p("/world/elem"),
            ty: Token::default()
        })),
        "{notices:?}"
    );
    assert!(
        notices.contains(&Notice::Added(Added {
            path: p("/world/elem/geo"),
            ty: Token::from("mesh")
        })),
        "{notices:?}"
    );
    assert!(merged.prim(&p("/world/elem")).is_defined());
}

#[test]
fn ancestors_of_a_root_exist() {
    let deep = RetainedSceneIndex::new();
    deep.add_prims([(p("/a/b/c"), prim("mesh", &[]))]);
    let merged = MergingSceneIndex::new();
    let recorder = RecordingObserver::on(merged.as_ref());
    merged.add_input_scene(deep.clone(), p("/a/b/c"));
    assert_eq!(
        recorder.take(),
        [
            Notice::Added(Added {
                path: p("/a"),
                ty: Token::default()
            }),
            Notice::Added(Added {
                path: p("/a/b"),
                ty: Token::default()
            }),
            Notice::Added(Added {
                path: p("/a/b/c"),
                ty: Token::from("mesh")
            }),
        ]
    );
    assert!(merged.prim(&p("/a")).is_defined());
    assert_eq!(merged.children(&p("/a")), [p("/a/b")]);
    assert_eq!(merged.children(&sdf_root()), [p("/a")]);
}

fn sdf_root() -> moho::openusd::sdf::Path {
    moho::openusd::sdf::Path::abs_root()
}
