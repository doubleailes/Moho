//! The scene index contract: prims, children and observers of added,
//! removed, dirtied and renamed prims, as `HdSceneIndexBase`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::data_source::ContainerHandle;
use crate::locator::LocatorSet;

/// A prim of a scene index: its type and its data (`HdSceneIndexPrim`).
#[derive(Clone, Default)]
pub struct Prim {
    /// The prim type token (`mesh`, `camera`, ...), empty for a prim with no
    /// Hydra type.
    pub ty: Token,
    /// The prim's data, `None` when nothing is at the path.
    pub data: Option<ContainerHandle>,
}

impl Prim {
    /// A defined prim.
    pub fn new(ty: impl Into<Token>, data: ContainerHandle) -> Self {
        Self {
            ty: ty.into(),
            data: Some(data),
        }
    }

    /// The prim answered where nothing is.
    pub fn undefined() -> Self {
        Self::default()
    }

    /// Whether a prim is at the path.
    pub fn is_defined(&self) -> bool {
        self.data.is_some()
    }
}

impl std::fmt::Debug for Prim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prim")
            .field("ty", &self.ty)
            .field("defined", &self.is_defined())
            .finish()
    }
}

/// A prim added, or resynced, with its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub path: sdf::Path,
    pub ty: Token,
}

/// A prim removed, with everything beneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub path: sdf::Path,
}

/// A prim whose data changed, for the locators it changed at. It does not
/// imply anything about the prim's descendants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirtied {
    pub path: sdf::Path,
    pub locators: LocatorSet,
}

/// A prim moved to another path, with everything beneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    pub old: sdf::Path,
    pub new: sdf::Path,
}

/// An observer of a scene index (`HdSceneIndexObserver`). Each call carries
/// a batch, and the index already reflects the change when it is called.
pub trait Observer {
    fn added(&self, sender: &dyn SceneIndex, entries: &[Added]);
    fn removed(&self, sender: &dyn SceneIndex, entries: &[Removed]);
    fn dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]);
    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]);
}

/// A scene index (`HdSceneIndexBase`).
pub trait SceneIndex {
    /// The prim at `path`, undefined when nothing is there.
    fn prim(&self, path: &sdf::Path) -> Prim;
    /// The paths of `path`'s children, in a stable order.
    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path>;
    /// Registers `observer`, held weakly: it stops receiving notices once
    /// dropped. It receives only the notices sent after it registers.
    fn add_observer(&self, observer: Weak<dyn Observer>);
}

/// A shared scene index handle.
pub type SceneIndexHandle = Rc<dyn SceneIndex>;

/// The observers of one scene index, and the sending of notices to them.
///
/// Sending copies the list first, so an observer may query the sender, or
/// register another observer, from inside its callback.
#[derive(Default)]
pub struct Observers(RefCell<Vec<Weak<dyn Observer>>>);

impl Observers {
    /// Registers `observer`.
    pub fn add(&self, observer: Weak<dyn Observer>) {
        self.0.borrow_mut().push(observer);
    }

    fn live(&self) -> Vec<Rc<dyn Observer>> {
        let mut list = self.0.borrow_mut();
        list.retain(|o| o.strong_count() > 0);
        list.iter().filter_map(Weak::upgrade).collect()
    }

    /// Whether any observer is registered and alive.
    pub fn is_empty(&self) -> bool {
        self.0.borrow().iter().all(|o| o.strong_count() == 0)
    }

    pub fn send_added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
        if !entries.is_empty() {
            for o in self.live() {
                o.added(sender, entries);
            }
        }
    }

    pub fn send_removed(&self, sender: &dyn SceneIndex, entries: &[Removed]) {
        if !entries.is_empty() {
            for o in self.live() {
                o.removed(sender, entries);
            }
        }
    }

    pub fn send_dirtied(&self, sender: &dyn SceneIndex, entries: &[Dirtied]) {
        if !entries.is_empty() {
            for o in self.live() {
                o.dirtied(sender, entries);
            }
        }
    }

    pub fn send_renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        if !entries.is_empty() {
            for o in self.live() {
                o.renamed(sender, entries);
            }
        }
    }
}

/// A scene index whose prims are added, removed and dirtied by hand
/// (`HdRetainedSceneIndex`), for tests and as the input of filters.
///
/// Adding a prim makes its missing ancestors present as undefined prims, so
/// that they list it among their children, as Hydra's does.
#[derive(Default)]
pub struct RetainedSceneIndex {
    prims: RefCell<BTreeMap<sdf::Path, Prim>>,
    observers: Observers,
}

impl RetainedSceneIndex {
    /// An empty retained scene index.
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    /// Adds (or replaces) prims, and notifies them added.
    pub fn add_prims(&self, prims: impl IntoIterator<Item = (sdf::Path, Prim)>) {
        let mut added = Vec::new();
        {
            let mut map = self.prims.borrow_mut();
            for (path, prim) in prims {
                for ancestor in path.strict_ancestors() {
                    map.entry(ancestor).or_default();
                }
                added.push(Added {
                    path: path.clone(),
                    ty: prim.ty.clone(),
                });
                map.insert(path, prim);
            }
        }
        self.observers.send_added(self, &added);
    }

    /// Removes prims with their subtrees, and notifies them removed.
    pub fn remove_prims(&self, paths: impl IntoIterator<Item = sdf::Path>) {
        let mut removed = Vec::new();
        {
            let mut map = self.prims.borrow_mut();
            for path in paths {
                map.retain(|p, _| !p.has_prefix(&path));
                removed.push(Removed { path });
            }
        }
        self.observers.send_removed(self, &removed);
    }

    /// Notifies prims dirtied, after the caller has changed what their data
    /// sources answer.
    pub fn dirty_prims(&self, entries: impl IntoIterator<Item = (sdf::Path, LocatorSet)>) {
        let dirtied: Vec<Dirtied> = entries
            .into_iter()
            .map(|(path, locators)| Dirtied { path, locators })
            .collect();
        self.observers.send_dirtied(self, &dirtied);
    }
}

impl SceneIndex for RetainedSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        self.prims.borrow().get(path).cloned().unwrap_or_default()
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.prims
            .borrow()
            .keys()
            .filter(|p| !p.is_abs_root() && p.parent().as_ref() == Some(path))
            .cloned()
            .collect()
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

/// Helpers for tests of scene indices.
pub mod testing {
    use super::*;

    /// One notice, as an observer received it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Notice {
        Added(Added),
        Removed(Removed),
        Dirtied(Dirtied),
        Renamed(Renamed),
    }

    /// An observer that records every notice in order.
    #[derive(Default)]
    pub struct RecordingObserver(RefCell<Vec<Notice>>);

    impl RecordingObserver {
        /// A recording observer registered on `index`. Keep the returned
        /// handle alive for as long as it should record.
        pub fn on(index: &dyn SceneIndex) -> Rc<Self> {
            let recorder = Rc::new(Self::default());
            let weak: Weak<dyn Observer> = Rc::downgrade(&(recorder.clone() as Rc<dyn Observer>));
            index.add_observer(weak);
            recorder
        }

        /// The notices recorded since the last call, which are cleared.
        pub fn take(&self) -> Vec<Notice> {
            std::mem::take(&mut *self.0.borrow_mut())
        }
    }

    impl Observer for RecordingObserver {
        fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
            self.0
                .borrow_mut()
                .extend(entries.iter().cloned().map(Notice::Added));
        }

        fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
            self.0
                .borrow_mut()
                .extend(entries.iter().cloned().map(Notice::Removed));
        }

        fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
            self.0
                .borrow_mut()
                .extend(entries.iter().cloned().map(Notice::Dirtied));
        }

        fn renamed(&self, _: &dyn SceneIndex, entries: &[Renamed]) {
            self.0
                .borrow_mut()
                .extend(entries.iter().cloned().map(Notice::Renamed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Notice, RecordingObserver};
    use super::*;
    use crate::data_source::retained::{RetainedContainer, value};
    use crate::locator::Locator;

    fn p(s: &str) -> sdf::Path {
        sdf::path(s).unwrap()
    }

    fn mesh() -> Prim {
        Prim::new(
            "mesh",
            RetainedContainer::new([("radius", value(1.0f64))]).into_handle(),
        )
    }

    #[test]
    fn prims_and_children() {
        let index = RetainedSceneIndex::new();
        index.add_prims([
            (p("/world/geo"), mesh()),
            (
                p("/world/cam"),
                Prim::new("camera", RetainedContainer::default().into_handle()),
            ),
        ]);
        let geo = index.prim(&p("/world/geo"));
        assert_eq!(geo.ty, "mesh");
        assert!(geo.is_defined());
        assert!(!index.prim(&p("/nowhere")).is_defined());
        assert_eq!(index.children(&p("/world")), [p("/world/cam"), p("/world/geo")]);
        assert_eq!(index.children(&sdf::Path::abs_root()), [p("/world")]);
        assert!(
            !index.prim(&p("/world")).is_defined(),
            "an implied ancestor is undefined"
        );
    }

    #[test]
    fn a_removal_covers_the_subtree() {
        let index = RetainedSceneIndex::new();
        index.add_prims([
            (p("/world/a"), mesh()),
            (p("/world/a/b"), mesh()),
            (p("/world/c"), mesh()),
        ]);
        let recorder = RecordingObserver::on(index.as_ref());
        index.remove_prims([p("/world/a")]);
        assert_eq!(
            recorder.take(),
            [Notice::Removed(Removed { path: p("/world/a") })]
        );
        assert!(!index.prim(&p("/world/a")).is_defined());
        assert!(!index.prim(&p("/world/a/b")).is_defined());
        assert!(index.prim(&p("/world/c")).is_defined());
    }

    #[test]
    fn a_dirty_entry_is_local() {
        let index = RetainedSceneIndex::new();
        index.add_prims([(p("/world"), mesh()), (p("/world/a"), mesh())]);
        let recorder = RecordingObserver::on(index.as_ref());
        index.dirty_prims([(p("/world"), LocatorSet::from(Locator::parse("xform")))]);
        assert_eq!(
            recorder.take(),
            [Notice::Dirtied(Dirtied {
                path: p("/world"),
                locators: LocatorSet::from(Locator::parse("xform")),
            })]
        );
    }

    #[test]
    fn observers_see_only_later_notices() {
        let index = RetainedSceneIndex::new();
        index.add_prims([(p("/a"), mesh())]);
        let recorder = RecordingObserver::on(index.as_ref());
        assert!(recorder.take().is_empty());
        index.add_prims([(p("/b"), mesh())]);
        assert_eq!(
            recorder.take(),
            [Notice::Added(Added {
                path: p("/b"),
                ty: Token::from("mesh"),
            })]
        );
    }

    #[test]
    fn dropped_observers_stop_receiving() {
        let index = RetainedSceneIndex::new();
        let recorder = RecordingObserver::on(index.as_ref());
        drop(recorder);
        index.add_prims([(p("/a"), mesh())]);
        assert!(index.observers.is_empty());
    }

    /// An observer that queries its sender from inside the callback.
    struct Querying(RefCell<Vec<bool>>);

    impl Observer for Querying {
        fn added(&self, sender: &dyn SceneIndex, entries: &[Added]) {
            for e in entries {
                self.0.borrow_mut().push(sender.prim(&e.path).is_defined());
            }
        }
        fn removed(&self, _: &dyn SceneIndex, _: &[Removed]) {}
        fn dirtied(&self, _: &dyn SceneIndex, _: &[Dirtied]) {}
        fn renamed(&self, _: &dyn SceneIndex, _: &[Renamed]) {}
    }

    #[test]
    fn an_observer_may_query_the_sender() {
        let index = RetainedSceneIndex::new();
        let observer = Rc::new(Querying(RefCell::new(Vec::new())));
        let weak: Weak<dyn Observer> = Rc::downgrade(&(observer.clone() as Rc<dyn Observer>));
        index.add_observer(weak);
        index.add_prims([(p("/a"), mesh())]);
        assert_eq!(*observer.0.borrow(), [true]);
    }
}
