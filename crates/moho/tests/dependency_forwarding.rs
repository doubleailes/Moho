#![allow(clippy::unwrap_used)]

mod common;

use common::p;
use moho::data_source::retained::{RetainedContainer, container};
use moho::filter::DependencyForwardingSceneIndex;
use moho::locator::{Locator, LocatorSet};
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Dirtied, Prim, RetainedSceneIndex, SceneIndex};
use moho::schema::DependencySchema;

/// A prim declaring `affected` depends on `on`'s `depended_on`.
fn declaring(on: &str, depended_on: &str, affected: &str) -> Prim {
    Prim::new(
        "thing",
        RetainedContainer::new([(
            "__dependencies",
            container([(
                "dep",
                DependencySchema::build(
                    Some(&p(on)),
                    &Locator::parse(depended_on),
                    &Locator::parse(affected),
                ),
            )]),
        )])
        .into_handle(),
    )
}

fn plain() -> Prim {
    Prim::new("thing", RetainedContainer::default().into_handle())
}

fn dirtied(path: &str, locator: &str) -> Notice {
    Notice::Dirtied(Dirtied {
        path: p(path),
        locators: LocatorSet::from(Locator::parse(locator)),
    })
}

#[test]
fn a_chained_dependency() {
    let input = RetainedSceneIndex::new();
    input.add_prims([
        (p("/a"), declaring("/b", "y", "x")),
        (p("/b"), declaring("/c", "z", "y")),
        (p("/c"), plain()),
    ]);
    let index = DependencyForwardingSceneIndex::new(input.clone());
    // Dependencies count once their declaring prim has been read.
    index.prim(&p("/a"));
    index.prim(&p("/b"));
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([(p("/c"), LocatorSet::from(Locator::parse("z")))]);
    assert_eq!(
        recorder.take(),
        [dirtied("/c", "z"), dirtied("/b", "y"), dirtied("/a", "x")]
    );
}

#[test]
fn an_unread_dependency_is_not_forwarded() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/a"), declaring("/b", "y", "x")), (p("/b"), plain())]);
    let index = DependencyForwardingSceneIndex::new(input.clone());
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([(p("/b"), LocatorSet::from(Locator::parse("y")))]);
    assert_eq!(recorder.take(), [dirtied("/b", "y")]);
}

#[test]
fn an_unrelated_locator_is_not_forwarded() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/a"), declaring("/b", "y", "x")), (p("/b"), plain())]);
    let index = DependencyForwardingSceneIndex::new(input.clone());
    index.prim(&p("/a"));
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([(p("/b"), LocatorSet::from(Locator::parse("w")))]);
    assert_eq!(recorder.take(), [dirtied("/b", "w")]);
}

#[test]
fn removing_a_depended_on_prim_dirties_its_dependents() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/a"), declaring("/b", "y", "x")), (p("/b"), plain())]);
    let index = DependencyForwardingSceneIndex::new(input.clone());
    index.prim(&p("/a"));
    let recorder = RecordingObserver::on(index.as_ref());
    input.remove_prims([p("/b")]);
    assert_eq!(
        recorder.take(),
        [
            Notice::Removed(moho::scene_index::Removed { path: p("/b") }),
            dirtied("/a", "x"),
        ]
    );
}

#[test]
fn readding_a_depended_on_prim_dirties_its_dependents() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/a"), declaring("/b", "y", "x")), (p("/b"), plain())]);
    let index = DependencyForwardingSceneIndex::new(input.clone());
    index.prim(&p("/a"));
    let recorder = RecordingObserver::on(index.as_ref());
    input.add_prims([(p("/b"), plain())]);
    let notices = recorder.take();
    assert_eq!(notices.last(), Some(&dirtied("/a", "x")), "{notices:?}");
}

#[test]
fn tracked_dependencies_are_blocked() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/a"), declaring("/b", "y", "x")), (p("/b"), plain())]);
    let index = DependencyForwardingSceneIndex::new(input);
    let data = index.prim(&p("/a")).data.unwrap();
    assert!(data.names().iter().any(|n| n == "__dependencies"));
    assert!(data.get(&"__dependencies".into()).is_none());
}

#[test]
fn a_dependency_cycle_ends() {
    let input = RetainedSceneIndex::new();
    input.add_prims([
        (p("/a"), declaring("/b", "y", "x")),
        (p("/b"), declaring("/a", "x", "y")),
    ]);
    let index = DependencyForwardingSceneIndex::new(input.clone());
    index.prim(&p("/a"));
    index.prim(&p("/b"));
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([(p("/a"), LocatorSet::from(Locator::parse("x")))]);
    assert_eq!(
        recorder.take(),
        [dirtied("/a", "x"), dirtied("/b", "y"), dirtied("/a", "x")]
    );
}
