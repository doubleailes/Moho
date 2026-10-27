//! `PrimManagingObserver`: a scene index observed as a set of managed prims,
//! one per prim, made by a prim factory, as
//! `HdsiPrimManagingSceneIndexObserver`.
//!
//! The observer asks its factory for a managed prim for every prim the scene
//! index holds when the observer is made, and for every added entry (a
//! resync replaces the prim at its path; a factory answering `None` leaves
//! no prim there). Each dirtied entry reaches the managed prim at its path
//! with its locator set, and a removed entry releases every managed prim at
//! or under its path. A renamed entry is a removal and an addition.
//!
//! This is what a Hydra 2.0 render delegate observes its (batched) terminal
//! scene index with (hdPrman's `renderDelegate.cpp` and
//! `rileyPrimFactory.cpp`): its managed prims create, update and release the
//! renderer's objects. Hydra guards `Dirty` for parallel sync; Moho's scene
//! indices are confined to one thread, so this observer is too.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use openusd::sdf;

use crate::scene_index::{
    Added, Dirtied, Observer, Removed, Renamed, SceneIndex, SceneIndexHandle, renamed_as_removed_and_added,
};

/// A prim the observer manages (`HdsiPrimManagingSceneIndexObserver::PrimBase`).
/// Releasing it (dropping the last handle) is how it learns it was removed.
pub trait ManagedPrim {
    /// The prim's data changed at `entry.locators`.
    fn dirty(&self, entry: &Dirtied, observer: &PrimManagingObserver);
}

/// Makes the managed prims (`PrimFactoryBase`).
pub trait PrimFactory {
    /// A managed prim for the added prim `entry`, `None` for a type the
    /// consumer does not manage.
    fn create_prim(&self, entry: &Added, observer: &PrimManagingObserver) -> Option<Rc<dyn ManagedPrim>>;
}

/// An observer turning a scene index's prims into managed prims.
pub struct PrimManagingObserver {
    index: SceneIndexHandle,
    factory: Option<Rc<dyn PrimFactory>>,
    prims: RefCell<BTreeMap<sdf::Path, Rc<dyn ManagedPrim>>>,
}

impl PrimManagingObserver {
    /// An observer of `index` making its prims with `factory` (none without
    /// one), starting with every prim `index` holds now. Keep the returned
    /// handle alive for as long as it should observe.
    pub fn new(index: SceneIndexHandle, factory: Option<Rc<dyn PrimFactory>>) -> Rc<Self> {
        let observer = Rc::new(Self {
            index: index.clone(),
            factory,
            prims: RefCell::new(BTreeMap::new()),
        });
        let weak: Weak<dyn Observer> = Rc::downgrade(&(observer.clone() as Rc<dyn Observer>));
        index.add_observer(weak);
        if let Some(factory) = observer.factory.clone() {
            let mut stack = vec![sdf::Path::abs_root()];
            while let Some(path) = stack.pop() {
                let ty = index.prim(&path).ty;
                stack.extend(index.children(&path).into_iter().rev());
                if let Some(prim) = factory.create_prim(
                    &Added {
                        path: path.clone(),
                        ty,
                    },
                    &observer,
                ) {
                    observer.prims.borrow_mut().insert(path, prim);
                }
            }
        }
        observer
    }

    /// The observed scene index.
    pub fn scene_index(&self) -> &SceneIndexHandle {
        &self.index
    }

    /// The managed prim at `path`, if any. Holding the handle keeps the prim
    /// alive past its removal.
    pub fn prim(&self, path: &sdf::Path) -> Option<Rc<dyn ManagedPrim>> {
        self.prims.borrow().get(path).cloned()
    }

    /// The paths of every managed prim, in path order.
    pub fn paths(&self) -> Vec<sdf::Path> {
        self.prims.borrow().keys().cloned().collect()
    }
}

impl Observer for PrimManagingObserver {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let Some(factory) = self.factory.clone() else {
            return;
        };
        for entry in entries {
            let replaced = match factory.create_prim(entry, self) {
                Some(prim) => self.prims.borrow_mut().insert(entry.path.clone(), prim),
                // The prim's type after a resync is not managed.
                None => self.prims.borrow_mut().remove(&entry.path),
            };
            // The previous prim is released here, outside the borrow.
            drop(replaced);
        }
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let mut released = Vec::new();
        {
            let mut prims = self.prims.borrow_mut();
            for entry in entries {
                let paths: Vec<sdf::Path> = prims
                    .range(entry.path.clone()..)
                    .map(|(path, _)| path)
                    .take_while(|path| path.as_str().starts_with(entry.path.as_str()))
                    .filter(|path| path.has_prefix(&entry.path))
                    .cloned()
                    .collect();
                for path in paths {
                    released.extend(prims.remove(&path));
                }
            }
        }
        drop(released);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        for entry in entries {
            let prim = self.prims.borrow().get(&entry.path).cloned();
            if let Some(prim) = prim {
                prim.dirty(entry, self);
            }
        }
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}
