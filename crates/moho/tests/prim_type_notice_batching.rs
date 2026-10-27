#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use common::p;
use moho::data_source::retained::RetainedContainer;
use moho::filter::{PrimTypeNoticeBatchingSceneIndex, PrimTypePriority};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::tf::Token;
use moho::scene_index::testing::{Notice, RecordingObserver};
use moho::scene_index::{Added, Dirtied, Prim, Removed, RetainedSceneIndex, SceneIndex};

fn prim(ty: &str) -> Prim {
    Prim::new(ty, RetainedContainer::default().into_handle())
}

fn added(path: &str, ty: &str) -> Notice {
    Notice::Added(Added {
        path: p(path),
        ty: Token::from(ty),
    })
}

fn dirtied(path: &str, locators: &[&str]) -> Notice {
    Notice::Dirtied(Dirtied {
        path: p(path),
        locators: locators.iter().map(|l| Locator::parse(l)).collect(),
    })
}

/// Materials before everything else.
struct MaterialsFirst;

impl PrimTypePriority for MaterialsFirst {
    fn priority(&self, prim_type: &Token) -> usize {
        usize::from(prim_type != "material")
    }

    fn num_priorities(&self) -> usize {
        2
    }
}

fn batching(input: &Rc<RetainedSceneIndex>) -> Rc<PrimTypeNoticeBatchingSceneIndex> {
    PrimTypeNoticeBatchingSceneIndex::new(input.clone(), Some(Rc::new(MaterialsFirst)))
}

#[test]
fn empty_before_the_first_flush() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/world/geo"), prim("mesh"))]);
    let index = batching(&input);
    let recorder = RecordingObserver::on(index.as_ref());
    assert!(!index.prim(&p("/world/geo")).is_defined());
    assert!(index.children(&p("/world")).is_empty());
    input.dirty_prims([(p("/world/geo"), LocatorSet::from(Locator::parse("xform")))]);
    assert!(recorder.take().is_empty(), "held until flushed");
    index.flush();
    assert!(index.prim(&p("/world/geo")).is_defined());
    // What the input held when the index was made comes as added, the
    // later dirt absorbed.
    assert_eq!(
        recorder.take(),
        [added("/", ""), added("/world", ""), added("/world/geo", "mesh")]
    );
}

#[test]
fn materials_before_geometry() {
    let input = RetainedSceneIndex::new();
    let index = batching(&input);
    index.flush();
    let recorder = RecordingObserver::on(index.as_ref());
    input.add_prims([(p("/world/geo"), prim("mesh"))]);
    input.add_prims([(p("/looks/red"), prim("material"))]);
    assert!(recorder.take().is_empty());
    index.flush();
    assert_eq!(
        recorder.take(),
        [added("/looks/red", "material"), added("/world/geo", "mesh")]
    );
}

#[test]
fn dirtied_entries_are_consolidated() {
    let input = RetainedSceneIndex::new();
    input.add_prims([
        (p("/world/geo"), prim("mesh")),
        (p("/looks/red"), prim("material")),
    ]);
    let index = batching(&input);
    index.flush();
    let recorder = RecordingObserver::on(index.as_ref());
    input.dirty_prims([(p("/world/geo"), LocatorSet::from(Locator::parse("xform")))]);
    input.dirty_prims([(p("/looks/red"), LocatorSet::from(Locator::parse("material")))]);
    input.dirty_prims([(
        p("/world/geo"),
        LocatorSet::from(Locator::parse("primvars/points")),
    )]);
    index.flush();
    assert_eq!(
        recorder.take(),
        [
            dirtied("/looks/red", &["material"]),
            dirtied("/world/geo", &["primvars/points", "xform"]),
        ]
    );
}

#[test]
fn removals_come_first_and_drop_what_they_cover() {
    let input = RetainedSceneIndex::new();
    input.add_prims([
        (p("/world/geo"), prim("mesh")),
        (p("/world/b"), prim("mesh")),
        (p("/other"), prim("mesh")),
    ]);
    let index = batching(&input);
    index.flush();
    let recorder = RecordingObserver::on(index.as_ref());
    input.add_prims([(p("/other"), prim("mesh"))]);
    input.dirty_prims([(p("/world/geo"), LocatorSet::from(Locator::parse("xform")))]);
    input.remove_prims([p("/world/b")]);
    input.remove_prims([p("/world")]);
    index.flush();
    assert_eq!(
        recorder.take(),
        [
            Notice::Removed(Removed { path: p("/world") }),
            added("/other", "mesh")
        ]
    );
}
