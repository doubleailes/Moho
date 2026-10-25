//! Stage changes recorded between two `apply_pending_updates` calls, as
//! `UsdImagingStageSceneIndex` queues `ObjectsChanged`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use openusd::sdf;
use openusd::usd::{CommittedChange, Stage, StageSink};

/// What changed on the stage since updates were last applied.
#[derive(Default)]
pub struct Pending {
    /// Prims whose subtree must be repopulated.
    pub resynced: BTreeSet<sdf::Path>,
    /// Per prim, the properties whose spec appeared or vanished.
    pub resynced_properties: BTreeMap<sdf::Path, BTreeSet<String>>,
    /// Per prim, the properties whose value or metadata changed.
    pub updated_properties: BTreeMap<sdf::Path, BTreeSet<String>>,
    /// Whether the stage's own metadata (the pseudo-root's) changed.
    pub stage_metadata: bool,
}

impl Pending {
    fn property(map: &mut BTreeMap<sdf::Path, BTreeSet<String>>, path: &sdf::Path) {
        if let Some((prim, name)) = path.split_property() {
            map.entry(prim).or_default().insert(name.to_owned());
        }
    }

    fn record(&mut self, resynced: &[sdf::Path], changed_info_only: &[sdf::Path]) {
        for path in resynced {
            if path.is_property() {
                Self::property(&mut self.resynced_properties, path);
            } else {
                self.resynced.insert(path.prim_path());
            }
        }
        for path in changed_info_only {
            // A prim's own info-only change (kind, documentation...) changes
            // nothing Moho exposes; a change that does (active, instanceable,
            // apiSchemas) arrives as a resync. The pseudo-root's is the
            // stage's metadata.
            if path.is_property() {
                Self::property(&mut self.updated_properties, path);
            } else if path.is_abs_root() {
                // Time range, rate, render settings, metrics: the scene
                // globals.
                self.stage_metadata = true;
            }
        }
    }

    /// The resynced prims, minimal in ancestors.
    pub fn resync_roots(&self) -> Vec<sdf::Path> {
        let mut roots: Vec<sdf::Path> = Vec::new();
        for path in &self.resynced {
            if !roots.iter().any(|r| path.has_prefix(r)) {
                roots.push(path.clone());
            }
        }
        roots
    }
}

/// The stage sink that records into a [`Pending`].
pub struct Recorder(pub Rc<RefCell<Pending>>);

impl StageSink for Recorder {
    fn after_commit(&self, _stage: &Stage, change: &CommittedChange<'_>) {
        self.0
            .borrow_mut()
            .record(change.resynced, change.changed_info_only);
    }

    fn load_rules_changed(&self, _stage: &Stage, resynced: &[sdf::Path]) {
        self.0.borrow_mut().record(resynced, &[]);
    }

    fn layer_muting_changed(&self, _stage: &Stage, _layer: &str, _muted: bool, resynced: &[sdf::Path]) {
        self.0.borrow_mut().record(resynced, &[]);
    }
}
