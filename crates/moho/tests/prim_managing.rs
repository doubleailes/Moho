#![allow(clippy::unwrap_used)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::p;
use moho::data_source::retained::RetainedContainer;
use moho::locator::{Locator, LocatorSet};
use moho::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
use moho::scene_index::{Added, Dirtied, Prim, RetainedSceneIndex};

/// A factory recording what it was asked for, making prims that record
/// their dirt.
#[derive(Default)]
struct Recording {
    created: RefCell<Vec<String>>,
    dirt: Rc<RefCell<Vec<(String, LocatorSet)>>>,
}

struct Recorded(Rc<RefCell<Vec<(String, LocatorSet)>>>);

impl ManagedPrim for Recorded {
    fn dirty(&self, entry: &Dirtied, _: &PrimManagingObserver) {
        self.0
            .borrow_mut()
            .push((entry.path.as_str().to_owned(), entry.locators.clone()));
    }
}

impl PrimFactory for Recording {
    fn create_prim(&self, entry: &Added, _: &PrimManagingObserver) -> Option<Rc<dyn ManagedPrim>> {
        self.created.borrow_mut().push(entry.path.as_str().to_owned());
        (entry.ty == "mesh").then(|| Rc::new(Recorded(self.dirt.clone())) as Rc<dyn ManagedPrim>)
    }
}

fn mesh() -> Prim {
    Prim::new("mesh", RetainedContainer::default().into_handle())
}

#[test]
fn added_dirtied_removed() {
    let input = RetainedSceneIndex::new();
    let factory = Rc::new(Recording::default());
    let observer = PrimManagingObserver::new(input.clone(), Some(factory.clone()));
    // Only the root, which the factory does not manage.
    assert!(observer.paths().is_empty());

    input.add_prims([(p("/world/geo"), mesh())]);
    assert_eq!(*factory.created.borrow(), ["/", "/world/geo"]);
    assert!(observer.prim(&p("/world/geo")).is_some());

    let points = LocatorSet::from(Locator::parse("primvars/points"));
    input.dirty_prims([(p("/world/geo"), points.clone())]);
    assert_eq!(*factory.dirt.borrow(), [("/world/geo".to_owned(), points)]);

    input.remove_prims([p("/world")]);
    assert!(observer.prim(&p("/world/geo")).is_none());
    assert!(observer.paths().is_empty());
}

#[test]
fn prims_held_when_made_are_managed() {
    let input = RetainedSceneIndex::new();
    input.add_prims([(p("/world/a"), mesh()), (p("/world/b"), mesh())]);
    let factory = Rc::new(Recording::default());
    let observer = PrimManagingObserver::new(input.clone(), Some(factory.clone()));
    assert_eq!(observer.paths(), [p("/world/a"), p("/world/b")]);
    // A resync to an unmanaged type releases the prim.
    input.add_prims([(
        p("/world/a"),
        Prim::new("camera", RetainedContainer::default().into_handle()),
    )]);
    assert_eq!(observer.paths(), [p("/world/b")]);
}
