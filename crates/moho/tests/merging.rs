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

/// An input holding the hierarchy, and another holding one element's
/// contents.
fn two_inputs() -> (Rc<RetainedSceneIndex>, Rc<RetainedSceneIndex>) {
    let scene = RetainedSceneIndex::new();
    scene.add_prims([
        (p("/world"), prim("", &[("a", 1)])),
        (p("/world/elem"), prim("", &[("fromScene", 1), ("shared", 1)])),
        (p("/world/other"), prim("", &[])),
    ]);
    let element = RetainedSceneIndex::new();
    element.add_prims([
        (p("/world"), prim("", &[("ignored", 1)])),
        (p("/world/elem"), prim("", &[("fromChunk", 2), ("shared", 2)])),
        (p("/world/elem/geo"), prim("mesh", &[])),
    ]);
    (scene, element)
}

#[test]
fn two_inputs_share_a_prim() {
    let (scene, element) = two_inputs();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(element.clone(), p("/world/elem"));
    merged.add_input_scene(scene.clone(), sdf_root());
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "shared"),
        Some(Value::Int(2)),
        "the element input is stronger"
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
    // The union of the inputs' children, in no specified order.
    let mut children = merged.children(&p("/world"));
    children.sort();
    assert_eq!(children, [p("/world/elem"), p("/world/other")]);
}

#[test]
fn an_input_comes_and_goes() {
    let (scene, element) = two_inputs();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(scene.clone(), sdf_root());
    let recorder = RecordingObserver::on(merged.as_ref());
    let element_handle: SceneIndexHandle = element.clone();
    merged.insert_input_scene(0, element_handle.clone(), p("/world/elem"));
    assert_eq!(
        recorder.take(),
        [
            Notice::Added(Added {
                path: p("/world/elem"),
                ty: Token::default()
            }),
            Notice::Added(Added {
                path: p("/world/elem/geo"),
                ty: Token::from("mesh")
            }),
        ],
        "the root and what the input lists under it, not its ancestors"
    );
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "shared"),
        Some(Value::Int(2))
    );

    merged.remove_input_scene(&element_handle);
    assert_eq!(
        recorder.take(),
        [
            Notice::Removed(Removed {
                path: p("/world/elem/geo")
            }),
            Notice::Added(Added {
                path: p("/world/elem"),
                ty: Token::default()
            }),
        ]
    );
    assert!(!merged.prim(&p("/world/elem/geo")).is_defined());
    assert_eq!(
        read(merged.as_ref(), "/world/elem", "shared"),
        Some(Value::Int(1)),
        "back to the scene input"
    );

    element.dirty_prims([(p("/world/elem/geo"), Default::default())]);
    assert!(
        recorder.take().is_empty(),
        "a removed input's notices are ignored"
    );
}

#[test]
fn an_undefined_root_is_added() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/w/geo"), prim("mesh", &[]))]);
    let merged = MergingSceneIndex::new();
    let recorder = RecordingObserver::on(merged.as_ref());
    // The input does not define `/w/x`, its root.
    merged.add_input_scene(input.clone(), p("/w/x"));
    assert_eq!(
        recorder.take(),
        [Notice::Added(Added {
            path: p("/w/x"),
            ty: Token::default()
        })]
    );
}

#[test]
fn a_notice_outside_a_root_is_forwarded() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/a"), prim("", &[])), (p("/b/x"), prim("mesh", &[]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(input.clone(), p("/a"));
    let recorder = RecordingObserver::on(merged.as_ref());
    input.dirty_prims([(p("/b/x"), Locator::parse("xform").into())]);
    input.add_prims([(p("/b/y"), prim("mesh", &[]))]);
    assert_eq!(
        recorder.take(),
        [
            Notice::Dirtied(Dirtied {
                path: p("/b/x"),
                locators: Locator::parse("xform").into()
            }),
            Notice::Added(Added {
                path: p("/b/y"),
                ty: Token::from("mesh")
            }),
        ],
        "forwarded unfiltered, as HdMergingSceneIndex does"
    );
}

#[test]
fn an_added_entry_takes_the_merged_type() {
    let strong = RetainedSceneIndex::new();
    strong.add_prims([(p("/a"), prim("", &[]))]);
    let weak = RetainedSceneIndex::new();
    weak.add_prims([(p("/a"), prim("", &[]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(strong.clone(), p("/a"));
    merged.add_input_scene(weak.clone(), p("/a"));
    let recorder = RecordingObserver::on(merged.as_ref());
    strong.add_prims([(p("/a/x"), prim("mesh", &[]))]);
    weak.add_prims([(p("/a/x"), prim("camera", &[]))]);
    // Outside every root, no input gives a type: the entry's is emptied.
    weak.add_prims([(p("/b"), prim("camera", &[]))]);
    assert_eq!(
        recorder.take(),
        [
            Notice::Added(Added {
                path: p("/a/x"),
                ty: Token::from("mesh")
            }),
            Notice::Added(Added {
                path: p("/a/x"),
                ty: Token::from("mesh")
            }),
            Notice::Added(Added {
                path: p("/b"),
                ty: Token::default()
            }),
        ]
    );
}

#[test]
fn a_weaker_input_removes_a_shared_prim() {
    let (scene, element) = two_inputs();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(element.clone(), p("/world/elem"));
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
    let other = RetainedSceneIndex::new();
    other.add_prims([(p("/x"), prim("", &[]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(other.clone(), p("/x"));
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
    let mut roots = merged.children(&sdf_root());
    roots.sort();
    assert_eq!(roots, [p("/a"), p("/x")]);
}

#[test]
fn a_single_input_answers_every_path() {
    let deep = RetainedSceneIndex::new();
    deep.add_prims([(p("/a/b/c"), prim("mesh", &[])), (p("/z"), prim("camera", &[]))]);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(deep.clone(), p("/a/b/c"));
    // As HdMergingSceneIndex: with one input, its prims are the merge's,
    // under its root or not.
    assert_eq!(merged.prim(&p("/z")).ty, "camera");
    assert_eq!(merged.children(&p("/a")), [p("/a/b")]);
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
    let added = |path: &str, ty: &str| {
        Notice::Added(Added {
            path: p(path),
            ty: Token::from(ty),
        })
    };
    assert_eq!(
        recorder.take(),
        [
            added("/", ""),
            added("/a", ""),
            added("/a/b", ""),
            added("/a/b/geo", "mesh")
        ],
        "every prim the input lists, defined or not"
    );

    // A second input holds them too: removing the first keeps them.
    let other = RetainedSceneIndex::new();
    other.add_prims([
        (sdf_root(), prim("", &[])),
        (p("/a"), prim("", &[])),
        (p("/a/b"), prim("", &[])),
        (p("/a/b/geo"), prim("mesh", &[])),
    ]);
    merged.add_input_scene(other.clone(), sdf_root());
    recorder.take();
    let handle: SceneIndexHandle = deep;
    merged.remove_input_scene(&handle);
    let mut notices = recorder.take();
    notices.sort_by_key(|n| format!("{n:?}"));
    assert_eq!(
        notices,
        [
            added("/", ""),
            added("/a", ""),
            added("/a/b", ""),
            added("/a/b/geo", "mesh")
        ]
    );
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
    let (scene, element) = two_inputs();
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(scene.clone(), sdf_root());
    let recorder = RecordingObserver::on(merged.as_ref());
    // Only the new input's own prims are added, as HdMergingSceneIndex
    // does.
    merged.add_input_scene(element.clone(), p("/world/elem"));
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

#[test]
fn a_rename_is_a_removal_and_an_addition() {
    let inner = RetainedSceneIndex::new();
    inner.add_prims([(p("/a/x"), prim("mesh", &[]))]);
    let input = Renaming::new(inner);
    let merged = MergingSceneIndex::new();
    merged.add_input_scene(input.clone(), p("/a"));
    let recorder = RecordingObserver::on(merged.as_ref());
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
}
