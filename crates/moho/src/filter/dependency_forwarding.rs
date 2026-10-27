//! `DependencyForwardingSceneIndex`: dirty notices forwarded along the
//! dependencies prims declare in `__dependencies`, as
//! `HdDependencyForwardingSceneIndex`.
//!
//! A prim's dependencies are read when the prim is read through this index
//! (and again when its `__dependencies` is dirtied), never by walking the
//! scene: a dependency counts once its declaring prim has been read. Then:
//!
//! - a prim dirtied at locators intersecting a depended-on locator dirties
//!   its dependents at their affected locators, transitively, each
//!   (prim, locator) once per notice;
//! - a depended-on prim added again (a resync) dirties its other dependents
//!   at every locator they declare on it;
//! - a depended-on prim removed dirties its dependents that are not removed
//!   with it likewise, and a removed or re-added dependent forgets what it
//!   declared.
//!
//! The index answers `__dependencies` as a block on a prim whose
//! dependencies it tracks: it has forwarded them, so observers need not.
//!
//! It is a filter a consumer adds over its terminal scene index (a Hydra
//! renderer adds it as a scene index plugin, arnold-usd at phase 1000), not
//! part of [`chain::standard`](crate::chain::standard).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;

use crate::data_source::Overlay;
use crate::data_source::retained::{RetainedContainer, block};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, Dirtied, Observer, Observers, Prim, Removed, Renamed, SceneIndex, SceneIndexHandle,
    renamed_as_removed_and_added,
};
use crate::schema::DependenciesSchema;
use crate::tokens::dependencies as t;

/// One declared dependency's locators (`_LocatorsEntry`).
struct LocatorsEntry {
    depended_on: Locator,
    affected: Locator,
}

/// What one affected prim declares on one depended-on prim
/// (`_AffectedPrimDependencyEntry`).
#[derive(Default)]
struct AffectedPrimDependency {
    entries: BTreeMap<Token, LocatorsEntry>,
    flagged_for_deletion: bool,
}

/// The prims one affected prim depends on (`_AffectedPrimToDependsOnPathsEntry`).
#[derive(Default)]
struct AffectedPrimEntry {
    depends_on: BTreeSet<sdf::Path>,
    flagged_for_deletion: bool,
}

/// A prim's declared dependencies, as read from its data.
type Declared = Vec<(Token, sdf::Path, Locator, Locator)>;

#[derive(Default)]
struct State {
    /// Depended-on prim to its dependents (`_dependedOnPrimToDependentsMap`).
    dependents: BTreeMap<sdf::Path, BTreeMap<sdf::Path, AffectedPrimDependency>>,
    /// Affected prim to the prims it depends on
    /// (`_affectedPrimToDependsOnPathsMap`).
    depends_on: BTreeMap<sdf::Path, AffectedPrimEntry>,
    potentially_deleted_depended_on: BTreeSet<sdf::Path>,
    potentially_deleted_affected: BTreeSet<sdf::Path>,
}

/// `(prim, locator)` pairs already dirtied in one notice (`_VisitedNodeSet`).
type Visited = BTreeSet<(sdf::Path, Locator)>;

fn dependencies_locator() -> Locator {
    Locator::new([t::DEPENDENCIES])
}

/// The dependencies `prim` at `path` declares, `None` when it has no
/// `__dependencies`.
fn declared(path: &sdf::Path, prim: &Prim) -> Option<Declared> {
    let schema = DependenciesSchema::get(prim.data.as_ref()?)?;
    Some(
        schema
            .entries()
            .into_iter()
            .map(|(name, dependency)| {
                let on = dependency.depended_on_prim_path().unwrap_or_else(|| path.clone());
                let depended_on = dependency.depended_on_data_source_locator().unwrap_or_default();
                let affected = dependency.affected_data_source_locator().unwrap_or_default();
                (name, on, depended_on, affected)
            })
            .collect(),
    )
}

impl State {
    fn reset(&mut self) {
        *self = State::default();
    }

    /// `_UpdateDependencies`.
    fn update(&mut self, path: &sdf::Path, declared: Option<Declared>) {
        let Some(declared) = declared else {
            return;
        };
        let entry = self.depends_on.entry(path.clone()).or_default();
        entry.flagged_for_deletion = false;
        for (name, on, depended_on, affected) in declared {
            entry.depends_on.insert(on.clone());
            let reverse = self
                .dependents
                .entry(on)
                .or_default()
                .entry(path.clone())
                .or_default();
            reverse.entries.insert(
                name,
                LocatorsEntry {
                    depended_on,
                    affected,
                },
            );
            reverse.flagged_for_deletion = false;
        }
    }

    /// `_ClearDependencies`: flags what `path` declares, for deletion at the
    /// end of the notice.
    fn clear(&mut self, path: &sdf::Path) {
        let Some(entry) = self.depends_on.get_mut(path) else {
            return;
        };
        entry.flagged_for_deletion = true;
        self.potentially_deleted_affected.insert(path.clone());
        for on in &entry.depends_on {
            if let Some(dependency) = self.dependents.get_mut(on).and_then(|m| m.get_mut(path)) {
                dependency.flagged_for_deletion = true;
                self.potentially_deleted_depended_on.insert(on.clone());
            }
        }
    }

    /// `RemoveDeletedEntries`.
    fn remove_deleted_entries(&mut self) {
        for on in std::mem::take(&mut self.potentially_deleted_depended_on) {
            if let Some(affected) = self.dependents.get_mut(&on) {
                affected.retain(|_, d| !d.flagged_for_deletion);
                if affected.is_empty() {
                    self.dependents.remove(&on);
                }
            }
        }
        for path in std::mem::take(&mut self.potentially_deleted_affected) {
            if self.depends_on.get(&path).is_some_and(|e| e.flagged_for_deletion) {
                self.depends_on.remove(&path);
            }
        }
    }

    /// `_PrimDirtied`: the dependents of `path` dirtied by `locators`,
    /// transitively.
    fn prim_dirtied(
        &self,
        path: &sdf::Path,
        locators: &LocatorSet,
        visited: &mut Visited,
        more: &mut Vec<Dirtied>,
        rebuild: &mut BTreeSet<sdf::Path>,
    ) {
        let deps = dependencies_locator();
        let Some(dependents) = self.dependents.get(path) else {
            return;
        };
        for (affected_path, dependency) in dependents {
            let mut affected = LocatorSet::new();
            for entry in dependency.entries.values() {
                if !locators.intersects(&entry.depended_on) {
                    continue;
                }
                if visited.insert((affected_path.clone(), entry.affected.clone())) {
                    affected.insert(entry.affected.clone());
                    // `__dependencies` itself affected: re-read them after
                    // the notice (once per prim).
                    let rebuilds = entry.affected == deps
                        || (entry.affected.intersects(&deps)
                            && visited.insert((affected_path.clone(), deps.clone())));
                    if rebuilds {
                        rebuild.insert(affected_path.clone());
                    }
                }
            }
            if !affected.is_empty() {
                more.push(Dirtied {
                    path: affected_path.clone(),
                    locators: affected.clone(),
                });
                self.prim_dirtied(affected_path, &affected, visited, more, rebuild);
            }
        }
    }

    /// Every locator `dependency` affects.
    fn all_affected(dependency: &AffectedPrimDependency) -> LocatorSet {
        dependency.entries.values().map(|e| e.affected.clone()).collect()
    }
}

/// A scene index forwarding dirty notices along `__dependencies`.
pub struct DependencyForwardingSceneIndex {
    input: SceneIndexHandle,
    me: Weak<DependencyForwardingSceneIndex>,
    state: RefCell<State>,
    observers: Observers,
}

impl DependencyForwardingSceneIndex {
    /// A dependency forwarding index over `input`.
    pub fn new(input: SceneIndexHandle) -> Rc<Self> {
        let index = Rc::new_cyclic(|me| Self {
            input: input.clone(),
            me: me.clone(),
            state: RefCell::new(State::default()),
            observers: Observers::default(),
        });
        let observer: Weak<dyn Observer> = index.me.clone();
        input.add_observer(observer);
        index
    }

    /// Clears and re-reads the dependencies of each of `paths`, then drops
    /// what was flagged for deletion.
    fn rebuild_and_collect(&self, rebuild: BTreeSet<sdf::Path>) {
        for path in rebuild {
            let declared = declared(&path, &self.input.prim(&path));
            let mut state = self.state.borrow_mut();
            state.clear(&path);
            state.update(&path, declared);
        }
        self.state.borrow_mut().remove_deleted_entries();
    }
}

impl SceneIndex for DependencyForwardingSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        let mut prim = self.input.prim(path);
        if !self.state.borrow().depends_on.contains_key(path) {
            let declared = declared(path, &prim);
            self.state.borrow_mut().update(path, declared);
        }
        if self.state.borrow().depends_on.contains_key(path)
            && let Some(data) = prim.data.take()
        {
            let blocked = RetainedContainer::new([(t::DEPENDENCIES, block())]).into_handle();
            prim.data = Some(Overlay::handle([blocked, data]));
        }
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.input.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}

impl Observer for DependencyForwardingSceneIndex {
    fn added(&self, _: &dyn SceneIndex, entries: &[Added]) {
        let mut visited = Visited::new();
        let mut more = Vec::new();
        let mut rebuild = BTreeSet::new();
        for entry in entries {
            self.state.borrow_mut().clear(&entry.path);
            let state = self.state.borrow();
            let Some(dependents) = state.dependents.get(&entry.path) else {
                continue;
            };
            for (affected_path, dependency) in dependents {
                if *affected_path == entry.path {
                    continue;
                }
                let affected = State::all_affected(dependency);
                if !affected.is_empty() {
                    more.push(Dirtied {
                        path: affected_path.clone(),
                        locators: affected.clone(),
                    });
                    state.prim_dirtied(affected_path, &affected, &mut visited, &mut more, &mut rebuild);
                }
            }
        }
        self.rebuild_and_collect(rebuild);
        self.observers.send_added(self, entries);
        self.observers.send_dirtied(self, &more);
    }

    fn removed(&self, _: &dyn SceneIndex, entries: &[Removed]) {
        if entries.first().is_some_and(|e| e.path.is_abs_root()) {
            self.state.borrow_mut().reset();
            self.observers.send_removed(self, entries);
            return;
        }
        let under_removed = |path: &sdf::Path| entries.iter().any(|e| path.has_prefix(&e.path));
        {
            let mut state = self.state.borrow_mut();
            let flagged: Vec<sdf::Path> = state
                .depends_on
                .keys()
                .filter(|p| under_removed(p))
                .cloned()
                .collect();
            for path in flagged {
                if let Some(entry) = state.depends_on.get_mut(&path) {
                    entry.flagged_for_deletion = true;
                }
                state.potentially_deleted_affected.insert(path);
            }
            if state.potentially_deleted_affected.len() == state.depends_on.len() {
                state.reset();
                drop(state);
                self.observers.send_removed(self, entries);
                return;
            }
        }
        let mut visited = Visited::new();
        let mut more = Vec::new();
        let mut rebuild = BTreeSet::new();
        // Dependents removed with what they depend on are flagged first, and
        // skipped below.
        {
            let mut state = self.state.borrow_mut();
            let mut depended_on_flagged = Vec::new();
            for (on, dependents) in state.dependents.iter_mut() {
                for (affected_path, dependency) in dependents.iter_mut() {
                    if under_removed(affected_path) {
                        dependency.flagged_for_deletion = true;
                        depended_on_flagged.push(on.clone());
                    }
                }
            }
            state.potentially_deleted_depended_on.extend(depended_on_flagged);
        }
        {
            let state = self.state.borrow();
            for (on, dependents) in &state.dependents {
                for (affected_path, dependency) in dependents {
                    if dependency.flagged_for_deletion {
                        continue;
                    }
                    for entry in entries {
                        if !on.has_prefix(&entry.path) {
                            continue;
                        }
                        let affected = State::all_affected(dependency);
                        if !affected.is_empty() {
                            more.push(Dirtied {
                                path: affected_path.clone(),
                                locators: affected.clone(),
                            });
                            state.prim_dirtied(
                                affected_path,
                                &affected,
                                &mut visited,
                                &mut more,
                                &mut rebuild,
                            );
                        }
                    }
                }
            }
        }
        self.rebuild_and_collect(rebuild);
        self.observers.send_removed(self, entries);
        self.observers.send_dirtied(self, &more);
    }

    fn dirtied(&self, _: &dyn SceneIndex, entries: &[Dirtied]) {
        let deps = dependencies_locator();
        let mut visited = Visited::new();
        let mut more = Vec::new();
        let mut rebuild = BTreeSet::new();
        {
            let state = self.state.borrow();
            for entry in entries {
                if entry.locators.intersects(&deps) && visited.insert((entry.path.clone(), deps.clone())) {
                    rebuild.insert(entry.path.clone());
                }
                state.prim_dirtied(
                    &entry.path,
                    &entry.locators,
                    &mut visited,
                    &mut more,
                    &mut rebuild,
                );
            }
        }
        self.rebuild_and_collect(rebuild);
        if more.is_empty() {
            self.observers.send_dirtied(self, entries);
        } else {
            let mut all = entries.to_vec();
            all.extend(more);
            self.observers.send_dirtied(self, &all);
        }
    }

    fn renamed(&self, sender: &dyn SceneIndex, entries: &[Renamed]) {
        renamed_as_removed_and_added(self, sender, entries);
    }
}
