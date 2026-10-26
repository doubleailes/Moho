//! `StageSceneIndex`: one openusd stage read as a scene index, in
//! `UsdImagingStageSceneIndex`'s shape.
//!
//! The index is built empty and handed a stage with
//! [`set_stage`](StageSceneIndex::set_stage). It reads whatever load set and
//! population mask the stage was opened with, and never loads or unloads
//! anything itself. Prims are those the default traversal reaches (active,
//! loaded, defined, not abstract); the contents of native instances are not
//! exposed here (the native-instancing filter does that).

pub mod adapter;
pub mod attribute;
pub mod camera;
pub mod collection;
pub mod curves;
pub mod generic;
pub mod globals;
pub mod imageable;
pub mod implicit;
pub mod instancing;
pub mod light;
pub mod material;
pub mod material_binding;
pub mod mesh;
pub mod pending;
pub mod point_instancer;
pub mod prim_info;
pub mod prim_origin;
pub mod primvars;
pub mod render;
pub mod table;
pub mod volume;
pub mod xform;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;
use openusd::usd::{self, PrimPredicate, PrimStatus, Stage, StageBuilder, StageSinkId, TimeCode};

use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{
    Added, CollectionMembership, Dirtied, Observer, Observers, Prim, Removed, SceneIndex,
};

use pending::{Pending, Recorder};

use adapter::{Adapter, AdapterRegistry};

/// A stage builder with openusd-schemas' registry installed, which typed
/// schemas and schema fallbacks need. Open stages for a `StageSceneIndex`
/// with it.
pub fn stage_builder() -> StageBuilder {
    Stage::builder().schema_registry(openusd_schemas::schema_registry())
}

/// State every data source of one stage scene index shares: the current
/// time, and the data that may vary over time.
#[derive(Default)]
pub struct Globals {
    time: Cell<Option<TimeCode>>,
    /// Per prim, the locators of data sources built so far whose value may
    /// vary over time (Hydra's `FlagAsTimeVarying`).
    time_varying: RefCell<BTreeMap<sdf::Path, LocatorSet>>,
}

impl Globals {
    /// The index's current time, `None` for the default time.
    pub fn time(&self) -> Option<TimeCode> {
        self.time.get()
    }

    /// Records that the data at `locator` on `path` may vary over time.
    pub fn flag_time_varying(&self, path: &sdf::Path, locator: Locator) {
        self.time_varying
            .borrow_mut()
            .entry(path.clone())
            .or_default()
            .insert(locator);
    }

    fn forget_under(&self, root: &sdf::Path) {
        self.time_varying.borrow_mut().retain(|p, _| !p.has_prefix(root));
    }
}

/// What an adapter knows about the prim it builds data for.
#[derive(Clone)]
pub struct PrimContext {
    pub stage: Stage,
    pub path: sdf::Path,
    pub globals: Rc<Globals>,
}

impl PrimContext {
    /// The USD prim.
    pub fn prim(&self) -> Option<usd::Prim> {
        self.stage.prim(&self.path).ok()
    }
}

/// A prim's container: the union of what its adapters contribute, the
/// first adapter answering a name winning.
struct PrimContainer {
    ctx: PrimContext,
    adapters: Vec<Rc<dyn Adapter>>,
}

impl Container for PrimContainer {
    fn names(&self) -> Vec<Token> {
        let mut names: Vec<Token> = Vec::new();
        for adapter in &self.adapters {
            for name in adapter.names(&self.ctx) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        self.adapters.iter().find_map(|a| a.get(&self.ctx, name))
    }
}

struct Current {
    stage: Stage,
    sink: StageSinkId,
    pending: Rc<RefCell<Pending>>,
}

/// One openusd stage read as a scene index.
pub struct StageSceneIndex {
    current: RefCell<Option<Current>>,
    globals: Rc<Globals>,
    adapters: AdapterRegistry,
    cache: RefCell<HashMap<sdf::Path, Prim>>,
    /// Every prim announced to observers and not since removed.
    populated: RefCell<BTreeSet<sdf::Path>>,
    /// Collection membership queries, until the stage changes.
    memberships: RefCell<HashMap<(sdf::Path, String), Option<usd::MembershipQuery>>>,
    /// Whether unloaded prims are prims of the index (UsdImaging's
    /// `includeUnloadedPrims`).
    include_unloaded: Cell<bool>,
    observers: Observers,
}

impl StageSceneIndex {
    /// An empty index with Moho's standard adapters.
    pub fn new() -> Rc<Self> {
        Self::with_adapters(AdapterRegistry::standard())
    }

    /// An empty index with the given adapters.
    pub fn with_adapters(adapters: AdapterRegistry) -> Rc<Self> {
        Rc::new(Self {
            current: RefCell::new(None),
            globals: Rc::new(Globals::default()),
            adapters,
            cache: RefCell::new(HashMap::new()),
            populated: RefCell::new(BTreeSet::new()),
            memberships: RefCell::new(HashMap::new()),
            include_unloaded: Cell::new(false),
            observers: Observers::default(),
        })
    }

    /// Makes unloaded prims (a payload's root, without what the payload
    /// brings) prims of the index, as UsdImaging's `includeUnloadedPrims`
    /// option does. A streamed scene's `LoadNone` input uses it, so that each
    /// chunk's root stays when the chunk is dropped. Set it before
    /// [`set_stage`](Self::set_stage).
    pub fn set_include_unloaded_prims(&self, include: bool) {
        self.include_unloaded.set(include);
    }

    /// The traversal predicate: active, defined, not abstract, and loaded
    /// unless unloaded prims are included.
    fn predicate(&self) -> PrimPredicate {
        if self.include_unloaded.get() {
            PrimPredicate::new(PrimStatus::ACTIVE | PrimStatus::DEFINED, PrimStatus::ABSTRACT)
        } else {
            PrimPredicate::DEFAULT
        }
    }

    /// The stage being read, if any.
    pub fn stage(&self) -> Option<Stage> {
        self.current.borrow().as_ref().map(|c| c.stage.clone())
    }

    /// Replaces the stage: every prim held is removed (one removal of the
    /// absolute root), then every prim of `stage` the default traversal
    /// reaches is added. Edits made to `stage` from now on are queued until
    /// [`apply_pending_updates`](Self::apply_pending_updates).
    pub fn set_stage(&self, stage: Stage) {
        let previous = self.current.borrow_mut().take();
        let had_stage = previous.is_some();
        if let Some(previous) = previous {
            previous.stage.remove_sink(previous.sink);
        }
        self.cache.borrow_mut().clear();
        self.populated.borrow_mut().clear();
        self.memberships.borrow_mut().clear();
        self.globals.forget_under(&sdf::Path::abs_root());
        if had_stage {
            self.observers.send_removed(
                self,
                &[Removed {
                    path: sdf::Path::abs_root(),
                }],
            );
        }
        let pending = Rc::new(RefCell::new(Pending::default()));
        let sink = stage.add_sink(Recorder(pending.clone()));
        *self.current.borrow_mut() = Some(Current { stage, sink, pending });
        let added = self.populate(&sdf::Path::abs_root());
        self.observers.send_added(self, &added);
    }

    /// The prims the default traversal reaches at and under `root` (the
    /// absolute root itself excluded), recorded as populated.
    fn populate(&self, root: &sdf::Path) -> Vec<Added> {
        let Some(stage) = self.stage() else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        if root.is_abs_root() {
            // A traversal error leaves the prims found so far: the index
            // shows what the stage could compose.
            let _ = stage.traverse(self.predicate(), |path| paths.push(path.clone()));
            // Then the prototypes, which the traversal composes by visiting
            // their instances: each prototype's subtree may hold nested
            // instances, registering more prototypes, until none is new.
            let mut done: BTreeSet<sdf::Path> = BTreeSet::new();
            loop {
                let fresh: Vec<sdf::Path> = stage
                    .prototypes()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|p| !done.contains(p))
                    .collect();
                if fresh.is_empty() {
                    break;
                }
                for prototype in fresh {
                    if let Ok(prim) = stage.prim(&prototype)
                        && let Ok(range) = prim.filtered_subtree(self.predicate())
                    {
                        paths.extend(range.filter_map(Result::ok).map(|p| p.path().clone()));
                    }
                    done.insert(prototype);
                }
            }
        } else if self.prim(root).is_defined()
            && let Ok(start) = stage.prim(root)
            && let Ok(range) = start.filtered_subtree(self.predicate())
        {
            paths.extend(range.filter_map(Result::ok).map(|p| p.path().clone()));
        }
        let added: Vec<Added> = paths
            .into_iter()
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.populated
            .borrow_mut()
            .extend(added.iter().map(|a| a.path.clone()));
        added
    }

    /// Turns every stage edit since the last call into notices: resynced
    /// prims removed and re-added (or added when new, removed when gone),
    /// and changed properties as their prims dirtied for the locators the
    /// adapters map them to. Property changes under a resynced prim are
    /// folded into its resync.
    pub fn apply_pending_updates(&self) {
        let Some(pending) = self
            .current
            .borrow()
            .as_ref()
            .map(|c| std::mem::take(&mut *c.pending.borrow_mut()))
        else {
            return;
        };
        self.memberships.borrow_mut().clear();
        let roots = pending.resync_roots();
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for root in &roots {
            let was_present = {
                let mut populated = self.populated.borrow_mut();
                let present = populated.contains(root);
                populated.retain(|p| !p.has_prefix(root));
                present
            };
            self.cache.borrow_mut().retain(|p, _| !p.has_prefix(root));
            self.globals.forget_under(root);
            let now = self.populate(root);
            if was_present {
                removed.push(Removed { path: root.clone() });
            }
            added.extend(now);
        }
        let mut dirty: BTreeMap<sdf::Path, LocatorSet> = BTreeMap::new();
        if pending.stage_metadata && !roots.iter().any(|r| r.is_abs_root()) {
            dirty
                .entry(sdf::Path::abs_root())
                .or_default()
                .insert_set(&LocatorSet::from_iter([
                    Locator::new([crate::tokens::scene_globals::SCENE_GLOBALS]),
                    Locator::new([crate::tokens::usd::STAGE_METADATA]),
                ]));
        }
        // A prim's own metadata changed: of what Moho exposes, only its kind
        // can change without a resync.
        //
        // Workaround, an upstream candidate in `ROADMAP.md`: openusd's
        // commit notice names info-only paths but not their changed fields
        // (C++ `GetChangedFields`), so every such prim is dirtied for its
        // kind, whichever field changed.
        for prim in &pending.updated_prims {
            if roots.iter().any(|r| prim.has_prefix(r)) || !self.populated.borrow().contains(prim) {
                continue;
            }
            dirty.entry(prim.clone()).or_default().insert(Locator::new([
                crate::tokens::usd_prim_info::USD_PRIM_INFO,
                crate::tokens::usd_prim_info::KIND,
            ]));
        }
        // A shader or node graph resynced (added, removed, replaced) changes
        // the network of the material that represents it.
        for root in &roots {
            if let Some(material) = self.enclosing_material(root)
                && !roots.iter().any(|r| material.has_prefix(r))
            {
                dirty
                    .entry(material)
                    .or_default()
                    .insert(Locator::new([crate::tokens::material::MATERIAL]));
            }
        }
        let properties = pending
            .resynced_properties
            .into_iter()
            .chain(pending.updated_properties);
        let mut by_prim: std::collections::BTreeMap<sdf::Path, BTreeSet<String>> = Default::default();
        for (prim, names) in properties {
            by_prim.entry(prim).or_default().extend(names);
        }
        for (prim, names) in by_prim {
            if roots.iter().any(|r| prim.has_prefix(r)) || !self.populated.borrow().contains(&prim) {
                continue;
            }
            let locators = self.invalidate(&prim, &names);
            if !locators.is_empty() {
                dirty.entry(prim.clone()).or_default().insert_set(&locators);
            }
            // A shader or node graph is represented by its material.
            if let Some(material) = self.representing_material(&prim) {
                dirty
                    .entry(material)
                    .or_default()
                    .insert(Locator::new([crate::tokens::material::MATERIAL]));
            }
        }
        let dirtied: Vec<Dirtied> = dirty
            .into_iter()
            .map(|(path, locators)| Dirtied { path, locators })
            .collect();
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
        self.observers.send_dirtied(self, &dirtied);
    }

    /// The nearest `Material` ancestor of a `Shader` or `NodeGraph` prim,
    /// which represents it.
    fn representing_material(&self, path: &sdf::Path) -> Option<sdf::Path> {
        let stage = self.stage()?;
        let usd_type = |p: &sdf::Path| -> Option<Token> { stage.prim(p).ok()?.type_name().ok().flatten() };
        let ty = usd_type(path)?;
        if ty != "Shader" && ty != "NodeGraph" {
            return None;
        }
        path.strict_ancestors_below_root()
            .find(|a| usd_type(a).is_some_and(|t| t == "Material"))
    }

    /// The nearest `Material` strictly above `path`, whatever `path` is now
    /// (it may be gone from the stage).
    fn enclosing_material(&self, path: &sdf::Path) -> Option<sdf::Path> {
        let stage = self.stage()?;
        path.strict_ancestors_below_root().find(|a| {
            stage
                .prim(a)
                .ok()
                .and_then(|p| p.type_name().ok().flatten())
                .is_some_and(|t| t == "Material")
        })
    }

    /// The index's current time, `None` for the default time.
    pub fn time(&self) -> Option<TimeCode> {
        self.globals.time()
    }

    /// Sets the index's time. A different time dirties every data source
    /// built so far whose value may vary over time; the same time notifies
    /// nothing.
    pub fn set_time(&self, time: TimeCode) {
        if self.globals.time() == Some(time) {
            return;
        }
        self.globals.time.set(Some(time));
        let dirtied: Vec<Dirtied> = self
            .globals
            .time_varying
            .borrow()
            .iter()
            .map(|(path, locators)| Dirtied {
                path: path.clone(),
                locators: locators.clone(),
            })
            .collect();
        self.observers.send_dirtied(self, &dirtied);
    }

    /// The locators a change to `names` on `path` dirties, from the prim's
    /// adapters.
    fn invalidate(&self, path: &sdf::Path, names: &BTreeSet<String>) -> LocatorSet {
        let mut locators = LocatorSet::new();
        let Some(stage) = self.stage() else {
            return locators;
        };
        let ctx = self.context(&stage, path);
        let Some(prim) = ctx.prim() else {
            return locators;
        };
        let usd_type = prim.type_name().ok().flatten().unwrap_or_default();
        let apis = prim.api_schemas().unwrap_or_default();
        for adapter in self.adapters.for_prim(usd_type.as_str(), &apis) {
            for name in names {
                locators.insert_set(&adapter.invalidate(&ctx, name));
            }
        }
        locators
    }

    fn context(&self, stage: &Stage, path: &sdf::Path) -> PrimContext {
        PrimContext {
            stage: stage.clone(),
            path: path.clone(),
            globals: self.globals.clone(),
        }
    }

    /// Whether the stage exposes `path` as a prim of this index.
    fn exposes(stage: &Stage, path: &sdf::Path, predicate: PrimPredicate) -> bool {
        if !stage.prim_matches(path, predicate).unwrap_or(false) {
            return false;
        }
        // Inside a native instance: the instancing filter exposes those.
        !path.strict_ancestors_below_root().any(|a| {
            stage
                .prim(&a)
                .ok()
                .is_some_and(|p| p.is_instance().unwrap_or(false))
        })
    }

    fn compute(&self, stage: &Stage, path: &sdf::Path) -> Prim {
        let ctx = self.context(stage, path);
        if path.is_abs_root() {
            let adapters = self.adapters.for_prim("", &[]);
            return Prim::new(Token::default(), Rc::new(PrimContainer { ctx, adapters }));
        }
        if !Self::exposes(stage, path, self.predicate()) {
            return Prim::undefined();
        }
        let Some(usd_prim) = ctx.prim() else {
            return Prim::undefined();
        };
        let usd_type = usd_prim.type_name().ok().flatten().unwrap_or_default();
        let apis = usd_prim.api_schemas().unwrap_or_default();
        let mut ty = self
            .adapters
            .for_type(usd_type.as_str())
            .map(|a| a.prim_type(&ctx))
            .unwrap_or_default();
        // A prim applying LightAPI with no light type of its own.
        if ty.as_str().is_empty() && apis.iter().any(|a| a == "LightAPI") {
            ty = Token::from(crate::tokens::prim_type::LIGHT);
        }
        let adapters = self.adapters.for_prim(usd_type.as_str(), &apis);
        Prim::new(ty, Rc::new(PrimContainer { ctx, adapters }))
    }
}

impl CollectionMembership for StageSceneIndex {
    fn includes(&self, collection_prim: &sdf::Path, name: &str, path: &sdf::Path) -> Option<bool> {
        let key = (collection_prim.clone(), name.to_owned());
        if !self.memberships.borrow().contains_key(&key) {
            let query = self.stage().and_then(|stage| {
                let prim = stage.prim(collection_prim).ok()?;
                usd::CollectionAPI::from_prim_unchecked(prim, name)
                    .membership_query()
                    .ok()
            });
            self.memberships.borrow_mut().insert(key.clone(), query);
        }
        self.memberships
            .borrow()
            .get(&key)
            .and_then(|q| q.as_ref().map(|q| q.is_included(path)))
    }
}

impl SceneIndex for StageSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        if let Some(prim) = self.cache.borrow().get(path) {
            return prim.clone();
        }
        let Some(stage) = self.stage() else {
            return Prim::undefined();
        };
        let prim = self.compute(&stage, path);
        self.cache.borrow_mut().insert(path.clone(), prim.clone());
        prim
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        let Some(stage) = self.stage() else {
            return Vec::new();
        };
        if !path.is_abs_root() && !self.prim(path).is_defined() {
            return Vec::new();
        }
        let mut children: Vec<sdf::Path> = stage
            .prim(path)
            .ok()
            .and_then(|p| p.filtered_children(self.predicate()).ok())
            .map(|children| children.iter().map(|c| c.path().clone()).collect())
            .unwrap_or_default();
        if path.is_abs_root() {
            // Prototypes are prims of this index too, as in UsdImaging; the
            // native-instancing filter re-roots them.
            children.extend(stage.prototypes().unwrap_or_default());
        }
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}
