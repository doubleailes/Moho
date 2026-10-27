//! `PrimTypeNoticeBatchingSceneIndex`: notices held until flushed, then
//! sent grouped by prim type priority, as
//! `HdsiPrimTypeNoticeBatchingSceneIndex`.
//!
//! The index consolidates what it holds: several dirtied entries for a path
//! become one with the union of their locators; an added entry absorbs the
//! dirtied entries for its path that come after it (and replaces those
//! before it); a removal discards the added and dirtied entries held at or
//! under its path, and a removal under another removal is dropped. A
//! [`flush`](PrimTypeNoticeBatchingSceneIndex::flush) sends every removal,
//! then the added and dirtied entries grouped by the priority a
//! [`PrimTypePriority`] gives their prim types, lower first: for each
//! priority, its added entries, then its dirtied entries.
//!
//! The index is empty until its first flush, which also sends every prim
//! its input held when the index was made as added. A Hydra 2.0 render
//! delegate wraps its terminal scene index in one and flushes it in its
//! `Update` (hdPrman's `renderDelegate.cpp`), so that, say, materials are
//! made before the geometry bound to them.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::locator::LocatorSet;
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};

/// The priority of each prim type
/// (`HdsiPrimTypeNoticeBatchingSceneIndex::PrimTypePriorityFunctor`).
pub trait PrimTypePriority {
    /// The priority of `prim_type`: lower is sent first. It must be less
    /// than [`num_priorities`](Self::num_priorities).
    fn priority(&self, prim_type: &Token) -> usize;
    /// One more than the highest priority ever given.
    fn num_priorities(&self) -> usize;
}

/// A held added or dirtied entry (`_PrimAddedOrDirtiedEntry`).
enum Held {
    Added(Token),
    Dirtied(LocatorSet),
}

/// A scene index batching its input's notices until flushed.
pub struct PrimTypeNoticeBatchingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<PrimTypeNoticeBatchingSceneIndex>,
    priority: Option<Rc<dyn PrimTypePriority>>,
    num_priorities: usize,
    populated: Cell<bool>,
    added_or_dirtied: RefCell<BTreeMap<sdf::Path, Held>>,
    /// No path in it is under another.
    removed: RefCell<BTreeSet<sdf::Path>>,
    observers: Observers,
}

impl PrimTypeNoticeBatchingSceneIndex {
    /// A batching index over `input`, sending prim types in the order
    /// `priority` gives (all at one priority without it). Every prim `input`
    /// holds now is held as added until the first flush.
    pub fn new(input: SceneIndexHandle, priority: Option<Rc<dyn PrimTypePriority>>) -> Rc<Self> {
        let num_priorities = priority.as_ref().map_or(1, |p| p.num_priorities().max(1));
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            priority,
            num_priorities,
            populated: Cell::new(false),
            added_or_dirtied: RefCell::new(BTreeMap::new()),
            removed: RefCell::new(BTreeSet::new()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        {
            let mut held = index.added_or_dirtied.borrow_mut();
            let mut stack = vec![sdf::Path::abs_root()];
            while let Some(path) = stack.pop() {
                let ty = input.prim(&path).ty;
                stack.extend(input.children(&path).into_iter().rev());
                held.insert(path, Held::Added(ty));
            }
        }
        index
    }

    fn priority_of(&self, prim_type: &Token) -> usize {
        match &self.priority {
            None => 0,
            // Hydra reports a coding error and uses the last priority.
            Some(p) => p.priority(prim_type).min(self.num_priorities - 1),
        }
    }

    /// Sends every notice held since the last flush: the removals, then the
    /// added and dirtied entries by priority. The index answers its input
    /// from the first flush on.
    pub fn flush(&self) {
        self.populated.set(true);
        let removed = std::mem::take(&mut *self.removed.borrow_mut());
        let held = std::mem::take(&mut *self.added_or_dirtied.borrow_mut());
        if self.observers.is_empty() {
            return;
        }
        let removed: Vec<Removed> = removed.into_iter().map(|path| Removed { path }).collect();
        self.observers.send_removed(self, &removed);
        let mut added: Vec<Vec<Added>> = (0..self.num_priorities).map(|_| Vec::new()).collect();
        let mut dirtied: Vec<Vec<Dirtied>> = (0..self.num_priorities).map(|_| Vec::new()).collect();
        for (path, entry) in held {
            match entry {
                Held::Added(ty) => {
                    let priority = self.priority_of(&ty);
                    added[priority].push(Added { path, ty });
                }
                Held::Dirtied(locators) => {
                    let priority = match &self.priority {
                        None => 0,
                        Some(_) => self.priority_of(&self.input.prim(&path).ty),
                    };
                    dirtied[priority].push(Dirtied { path, locators });
                }
            }
        }
        for (added, dirtied) in added.iter().zip(&dirtied) {
            self.observers.send_added(self, added);
            self.observers.send_dirtied(self, dirtied);
        }
    }
}

impl SceneIndex for PrimTypeNoticeBatchingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if !self.populated.get() {
            return Prim::undefined();
        }
        self.input.prim(path)
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        if !self.populated.get() {
            return Vec::new();
        }
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

/// The paths at or under `root` among `from_root`, the paths from `root` on
/// in path order: their text begins with `root`'s, so they come first.
fn under<'a>(from_root: impl Iterator<Item = &'a sdf::Path>, root: &sdf::Path) -> Vec<sdf::Path> {
    from_root
        .take_while(|p| p.as_str().starts_with(root.as_str()))
        .filter(|p| p.has_prefix(root))
        .cloned()
        .collect()
}

impl Observer for PrimTypeNoticeBatchingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let mut held = self.added_or_dirtied.borrow_mut();
        for entry in entries {
            held.insert(entry.path.clone(), Held::Added(entry.ty.clone()));
        }
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        let mut held = self.added_or_dirtied.borrow_mut();
        let mut removed = self.removed.borrow_mut();
        for entry in entries {
            for path in under(held.range(entry.path.clone()..).map(|(k, _)| k), &entry.path) {
                held.remove(&path);
            }
            if entry.path.ancestors().any(|a| removed.contains(&a)) {
                continue;
            }
            for path in under(removed.range(entry.path.clone()..), &entry.path) {
                removed.remove(&path);
            }
            removed.insert(entry.path.clone());
        }
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let mut held = self.added_or_dirtied.borrow_mut();
        for entry in entries {
            match held
                .entry(entry.path.clone())
                .or_insert_with(|| Held::Dirtied(LocatorSet::new()))
            {
                // An added entry is stronger.
                Held::Added(_) => {}
                Held::Dirtied(locators) => locators.insert_set(&entry.locators),
            }
        }
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}
