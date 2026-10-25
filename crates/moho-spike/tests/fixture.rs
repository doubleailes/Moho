#![allow(clippy::unwrap_used)]

use moho_spike::fixture::{self, TempDir};
use openusd::sdf;
use openusd::usd::{InitialLoadSet, PrimPredicate, Stage};

#[test]
fn fixture_loads_eight_chunks_and_every_prim() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let stage = Stage::builder()
        .load(InitialLoadSet::LoadAll)
        .open(root.to_str().unwrap())
        .unwrap();
    assert!(
        stage.composition_errors().is_empty(),
        "{:?}",
        stage.composition_errors()
    );

    let world = stage.prim("/world").unwrap();
    assert_eq!(world.children().unwrap().len(), fixture::ELEMENTS);

    let mut prims = 0;
    stage.traverse(PrimPredicate::DEFAULT, |_| prims += 1).unwrap();
    assert_eq!(prims, fixture::PRIM_COUNT);

    // The payload's binding target is remapped into stage namespace.
    let mesh = stage.prim(fixture::animated_mesh_path(3)).unwrap();
    let targets = mesh.relationship("material:binding").targets().unwrap();
    assert_eq!(targets, [sdf::path("/world/element_3/Looks/mat").unwrap()]);
    assert!(mesh.attribute("points").might_be_time_varying().unwrap());
}

#[test]
fn fixture_unloaded_holds_only_the_hierarchy() {
    let dir = TempDir::new().unwrap();
    let root = fixture::write(dir.path()).unwrap();
    let stage = Stage::builder()
        .load(InitialLoadSet::LoadNone)
        .open(root.to_str().unwrap())
        .unwrap();
    let mut prims = 0;
    stage.traverse(PrimPredicate::DEFAULT, |_| prims += 1).unwrap();
    // `/world` alone: an unloaded element is not loaded, so the default
    // predicate rejects it.
    assert_eq!(prims, 1);
    assert_eq!(
        stage.prim("/world").unwrap().children().unwrap().len(),
        fixture::ELEMENTS
    );
}
