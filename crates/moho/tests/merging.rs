#![allow(clippy::unwrap_used)]

mod common;

use std::rc::{Rc, Weak};

use common::p;
use moho::data_source::locate;
use moho::data_source::retained::{RetainedContainer, value};
use moho::filter::MergingSceneIndex;
use moho::locator::Locator;
use moho::openusd::sdf::Value;
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, RetainedSceneIndex, SceneIndex,
    SceneIndexHandle,
};

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
        [Notice::Added(Added {
            path: p("/a/b/c"),
            ty: Token::from("mesh")
        })],
        "the ancestors are not notified, as HdMergingSceneIndex's are not"
    );
    assert!(merged.prim(&p("/a")).is_defined());
    assert_eq!(merged.children(&p("/a")), [p("/a/b")]);
    assert_eq!(merged.children(&sdf_root()), [p("/a")]);
}

fn sdf_root() -> moho::openusd::sdf::Path {
    moho::openusd::sdf::Path::abs_root()
}

/// A retained index that can also announce renames, which
/// `RetainedSceneIndex` never does.
struct Renaming {
    inner: Rc<RetainedSceneIndex>,
    observers: Observers,
}

impl Renaming {
    fn new(inner: Rc<RetainedSceneIndex>) -> Rc<Self> {
        Rc::new(Self {
            inner,
            observers: Observers::default(),
        })
    }

    /// Moves `old` (a leaf) to `new` and announces the rename.
    fn rename(&self, old: &str, new: &str) {
        let prim = self.inner.prim(&p(old));
        self.inner.remove_prims([p(old)]);
        self.inner.add_prims([(p(new), prim)]);
        self.observers.send_renamed(
            self,
            &[Renamed {
                old: p(old),
                new: p(new),
            }],
        );
    }
}

impl SceneIndex for Renaming {
    fn prim(&self, path: &moho::openusd::sdf::Path) -> Prim {
        self.inner.prim(path)
    }

    fn children(&self, path: &moho::openusd::sdf::Path) -> Vec<moho::openusd::sdf::Path> {
        self.inner.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

#[test]
fn renames_outside_a_root_stay_hidden() {
    let inner = RetainedSceneIndex::new();
    inner.add_prims([
        (p("/a/x"), prim("mesh", &[])),
        (p("/hidden/y"), prim("mesh", &[])),
    ]);
    let input = Renaming::new(inner);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(input.clone(), p("/a"));
    let recorder = RecordingObserver::on(merged.as_ref());

    input.rename("/hidden/y", "/hidden/z");
    assert_eq!(recorder.take(), [], "neither side is under /a");

    input.rename("/a/x", "/a/w");
    assert_eq!(
        recorder.take(),
        [
            Notice::Removed(Removed { path: p("/a/x") }),
            Notice::Added(Added {
                path: p("/a/w"),
                ty: Token::from("mesh")
            }),
        ]
    );

    input.rename("/a/w", "/hidden/w");
    assert_eq!(
        recorder.take(),
        [Notice::Removed(Removed { path: p("/a/w") })],
        "moved out of /a: removed only"
    );
}

#[test]
fn an_index_added_at_two_roots_reaches_both() {
    let inner = RetainedSceneIndex::new();
    inner.add_prims([(p("/a"), prim("", &[])), (p("/b"), prim("", &[]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(inner.clone(), p("/a"));
    merged.add_input_scene(inner.clone(), p("/b"));
    let recorder = RecordingObserver::on(merged.as_ref());
    inner.add_prims([(p("/b/geo"), prim("mesh", &[]))]);
    inner.dirty_prims([(p("/b/geo"), Locator::parse("x").into())]);
    assert_eq!(
        recorder.take(),
        [
            Notice::Added(Added {
                path: p("/b/geo"),
                ty: Token::from("mesh")
            }),
            Notice::Dirtied(Dirtied {
                path: p("/b/geo"),
                locators: Locator::parse("x").into()
            }),
        ],
        "once each, under the second root"
    );
}

#[test]
fn prims_under_undefined_ancestors_are_merged() {
    // `/a` and `/a/b` are implied, undefined ancestors of the mesh.
    let deep = RetainedSceneIndex::new();
    deep.add_prims([(p("/a/b/geo"), prim("mesh", &[]))]);
    let merged = MergingSceneIndex::new();
    let recorder = RecordingObserver::on(merged.as_ref());
    merged.add_input_scene(deep.clone(), sdf_root());
    assert_eq!(
        recorder.take(),
        [Notice::Added(Added {
            path: p("/a/b/geo"),
            ty: Token::from("mesh")
        })]
    );

    // A second input holds the mesh too: removing the first keeps it.
    let other = RetainedSceneIndex::new();
    other.add_prims([(p("/a/b/geo"), prim("mesh", &[]))]);
    merged.add_input_scene(other.clone(), sdf_root());
    recorder.take();
    let handle: SceneIndexHandle = deep;
    merged.remove_input_scene(&handle);
    assert_eq!(
        recorder.take(),
        [Notice::Added(Added {
            path: p("/a/b/geo"),
            ty: Token::from("mesh")
        })]
    );
}

#[test]
fn merged_children_are_in_input_order() {
    let strong = RetainedSceneIndex::new();
    strong.add_prims([(p("/w/b"), prim("", &[])), (p("/w/a"), prim("", &[]))]);
    let weak = RetainedSceneIndex::new();
    weak.add_prims([(p("/w/a"), prim("", &[])), (p("/w/c"), prim("", &[]))]);
    let deep = RetainedSceneIndex::new();
    deep.add_prims([(p("/w/d/e"), prim("", &[]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(deep.clone(), p("/w/d/e"));
    merged.add_input_scene(strong.clone(), sdf_root());
    merged.add_input_scene(weak.clone(), sdf_root());
    // The deep input is strongest, but implied prims come after every
    // input's own children.
    let strong_order = strong.children(&p("/w"));
    let weak_order = weak.children(&p("/w"));
    let mut expected: Vec<_> = strong_order.clone();
    for c in weak_order {
        if !expected.contains(&c) {
            expected.push(c);
        }
    }
    expected.push(p("/w/d"));
    assert_eq!(merged.children(&p("/w")), expected);
}

/// An input mounted at `/` can change the root's data (the scene globals):
/// adding or removing it notifies the root.
#[test]
fn an_input_at_the_root_notifies_the_root() {
    let weak = RetainedSceneIndex::new();
    weak.add_prims([(sdf_root(), prim("", &[("startTimeCode", 1)]))]);
    let strong = RetainedSceneIndex::new();
    strong.add_prims([(sdf_root(), prim("", &[("startTimeCode", 1001)]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(weak.clone(), sdf_root());
    let recorder = RecordingObserver::on(merged.as_ref());
    merged.insert_input_scene(0, strong.clone(), sdf_root());
    let root_added = |notices: &[Notice]| {
        notices
            .iter()
            .any(|n| matches!(n, Notice::Added(a) if a.path == sdf_root()))
    };
    assert!(root_added(&recorder.take()));
    assert_eq!(
        read(merged.as_ref(), "/", "startTimeCode"),
        Some(Value::Int(1001))
    );
    let handle: SceneIndexHandle = strong;
    merged.remove_input_scene(&handle);
    assert!(root_added(&recorder.take()), "the root's data changed back");
    assert_eq!(read(merged.as_ref(), "/", "startTimeCode"), Some(Value::Int(1)));
}

#[test]
fn an_input_does_not_re_add_its_ancestors() {
    let (scene, chunk) = scene_and_chunk();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(scene.clone(), sdf_root());
    let recorder = RecordingObserver::on(merged.as_ref());
    // Only the new input's own prims are added, as HdMergingSceneIndex
    // does.
    merged.add_input_scene(chunk.clone(), p("/world/elem"));
    let added: Vec<String> = recorder
        .take()
        .into_iter()
        .filter_map(|n| match n {
            Notice::Added(a) => Some(a.path.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(added, ["/world/elem", "/world/elem/geo"]);
}

#[test]
fn many_inputs_are_found_by_path() {
    let merged = MergingSceneIndex::new();
    let mut inputs = Vec::new();
    for i in 0..8 {
        let input = RetainedSceneIndex::new();
        let root = format!("/r{i}/deep");
        input.add_prims([
            (p(&root), prim("", &[("n", i)])),
            (p(&format!("{root}/geo")), prim("mesh", &[])),
        ]);
        merged.add_input_scene(input.clone(), p(&root));
        inputs.push(input);
    }
    // A stronger input at the root, inserted first.
    let top = RetainedSceneIndex::new();
    top.add_prims([(p("/r3/deep"), prim("", &[("n", 100)]))]);
    merged.insert_input_scene(0, top.clone(), sdf_root());
    assert_eq!(read(merged.as_ref(), "/r3/deep", "n"), Some(Value::Int(100)));
    assert_eq!(read(merged.as_ref(), "/r5/deep", "n"), Some(Value::Int(5)));
    assert_eq!(merged.prim(&p("/r5/deep/geo")).ty, "mesh");
    assert!(merged.prim(&p("/r5")).is_defined(), "an ancestor of a root");
    assert!(!merged.prim(&p("/elsewhere")).is_defined());
    assert_eq!(merged.children(&p("/r5")), [p("/r5/deep")]);
    let roots: Vec<_> = merged.children(&sdf_root());
    assert_eq!(roots.len(), 8);
    let removed: SceneIndexHandle = inputs.remove(5);
    merged.remove_input_scene(&removed);
    assert!(!merged.prim(&p("/r5/deep")).is_defined());
    assert!(!merged.prim(&p("/r5")).is_defined());
    assert_eq!(merged.children(&sdf_root()).len(), 7);
}
