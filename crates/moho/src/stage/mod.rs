//! `StageSceneIndex`: one openusd stage read as a scene index, in
//! `UsdImagingStageSceneIndex`'s shape.
//!
//! The index is built empty and handed a stage with
//! [`set_stage`](StageSceneIndex::set_stage). It reads whatever load set and
//! population mask the stage was opened with, and never loads or unloads
//! anything itself. Prims are those UsdImaging's traversal reaches (active,
//! loaded, not abstract; `over` prims included); the contents of native
//! instances are not exposed here (the native-instancing filter does that).

pub mod adapter;
pub mod attribute;
pub mod camera;
pub mod collection;
pub mod curves;
pub mod generic;
pub mod geom_subset;
pub mod globals;
pub mod imageable;
pub mod implicit;
pub mod light;
pub mod material;
pub mod material_binding;
pub mod mesh;
pub mod model;
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
use std::sync::Arc;

use openusd::sdf;
use openusd::tf::Token;
use openusd::usd::{self, PrimPredicate, PrimStatus, Stage, StageBuilder, StageSinkId, TimeCode};

use crate::data_source::{Container, DataSource};
use crate::locator::{Locator, LocatorSet};
use crate::scene_index::{Added, Dirtied, Observer, Observers, Prim, Removed, SceneIndex};

use pending::{Pending, Recorder};

use adapter::{Adapter, AdapterRegistry};

/// A stage builder with openusd-schemas' registry installed, which typed
/// schemas and schema fallbacks need. Open stages for a `StageSceneIndex`
/// with it.
pub fn stage_builder() -> StageBuilder {
    Stage::builder().schema_registry(openusd_schemas::schema_registry())
}

/// State every data source of one stage scene index shares: the current
/// time, and the data that may vary over time
/// (`UsdImagingStageSceneIndex::_StageGlobals`).
pub struct Globals {
    time: Cell<TimeCode>,
    /// Per prim, the locators of data sources built so far whose value may
    /// vary over time (Hydra's `FlagAsTimeVarying`).
    time_varying: RefCell<BTreeMap<sdf::Path, LocatorSet>>,
}

impl Default for Globals {
    /// Time code 0, a default-constructed `UsdTimeCode`'s, as
    /// `_StageGlobals` starts.
    fn default() -> Self {
        Self {
            time: Cell::new(TimeCode::new(0.0)),
            time_varying: RefCell::default(),
        }
    }
}

impl Globals {
    /// The index's current time.
    pub fn time(&self) -> TimeCode {
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
    cache: RefCell<sdf::PathMap<Prim>>,
    /// Each prim's children, as `children` answers them until a resync.
    children: RefCell<sdf::PathMap<Vec<sdf::Path>>>,
    /// Every prim announced to observers and not since removed.
    populated: RefCell<BTreeSet<sdf::Path>>,
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
            cache: RefCell::default(),
            children: RefCell::default(),
            populated: RefCell::new(BTreeSet::new()),
            include_unloaded: Cell::new(false),
            observers: Observers::default(),
        })
    }

    /// Makes unloaded prims (a payload's root, without what the payload
    /// brings) prims of the index, as UsdImaging's `includeUnloadedPrims`
    /// option does. Set it before
    /// [`set_stage`](Self::set_stage).
    pub fn set_include_unloaded_prims(&self, include: bool) {
        self.include_unloaded.set(include);
    }

    /// UsdImaging's traversal predicate: active, not abstract, and loaded
    /// unless unloaded prims are included. A prim need not be defined, so
    /// `over` prims and their descendants are exposed (prototypes a point
    /// instancer targets often live under one); the point instancing
    /// filter makes what lies under an `over` unrenderable.
    fn predicate(&self) -> PrimPredicate {
        if self.include_unloaded.get() {
            PrimPredicate::new(PrimStatus::ACTIVE, PrimStatus::ABSTRACT)
        } else {
            PrimPredicate::new(PrimStatus::ACTIVE | PrimStatus::LOADED, PrimStatus::ABSTRACT)
        }
    }

    /// The stage being read, if any.
    pub fn stage(&self) -> Option<Stage> {
        self.current.borrow().as_ref().map(|c| c.stage.clone())
    }

    /// Replaces the stage: every prim held is removed (one removal of the
    /// absolute root) and the time is reset to `TimeCode::EARLIEST` (as
    /// `_StageGlobals::Clear`), then every prim of `stage` the default
    /// traversal reaches is added. Edits made to `stage` from now on are queued until
    /// [`apply_pending_updates`](Self::apply_pending_updates).
    pub fn set_stage(&self, stage: Stage) {
        let previous = self.current.borrow_mut().take();
        let had_stage = previous.is_some();
        if let Some(previous) = previous {
            previous.stage.remove_sink(previous.sink);
        }
        self.cache.borrow_mut().clear();
        self.children.borrow_mut().clear();
        self.populated.borrow_mut().clear();
        self.globals.forget_under(&sdf::Path::abs_root());
        if had_stage {
            self.observers.send_removed(
                self,
                &[Removed {
                    path: sdf::Path::abs_root(),
                }],
            );
            // `_StageGlobals::Clear`.
            self.globals.time.set(TimeCode::EARLIEST);
        }
        let pending = Rc::new(RefCell::new(Pending::default()));
        let sink = stage.add_sink(Recorder(pending.clone()));
        *self.current.borrow_mut() = Some(Current { stage, sink, pending });
        let added = self.populate(&sdf::Path::abs_root());
        self.observers.send_added(self, &added);
    }

    /// Forgets the prims and children held at and under `root`, and the
    /// children of its parent and of the absolute root (whose children
    /// include the prototypes, which any resync may add or retire).
    fn forget_under(&self, root: &sdf::Path) {
        self.cache.borrow_mut().retain(|p, _| !p.has_prefix(root));
        let mut children = self.children.borrow_mut();
        children.retain(|p, _| !p.has_prefix(root));
        if let Some(parent) = root.parent() {
            children.remove(&parent);
        }
        children.remove(&sdf::Path::abs_root());
    }

    /// The prims the default traversal reaches at and under `root`,
    /// recorded as populated. Populating from the absolute root adds the
    /// absolute root first, typeless, as `_PopulateSubtree` does for the
    /// pseudo-root ("we have to make sure the prim at the absolute root path
    /// is added"), so filters built before the stage was set read its data
    /// again.
    fn populate(&self, root: &sdf::Path) -> Vec<Added> {
        let Some(stage) = self.stage() else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        if root.is_abs_root() {
            paths.push(sdf::Path::abs_root());
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
        let added: Vec<Added> = prune_material_descendants(&stage, paths)
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
    /// prims added (again, or for the first time), each prim that was held
    /// and is not re-added removed,
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
        // A prim's own field may reorder its children (`primOrder`): the
        // children held for it are read again, without a notice, as they
        // were on every call before they were held.
        if !pending.info_changed.is_empty() {
            let mut children = self.children.borrow_mut();
            for path in &pending.info_changed {
                children.remove(path);
            }
        }
        let mut roots = pending.resync_roots();
        // The prototypes populated before this batch, needed only when it
        // resyncs one (rare): not computed on every update.
        let touches_prototype = {
            let populated = self.populated.borrow();
            roots
                .iter()
                .any(|r| r.is_root_prim() && r.as_str().starts_with("/__Prototype_") && populated.contains(r))
        };
        let known_prototypes = if touches_prototype {
            self.populated_prototypes()
        } else {
            BTreeSet::new()
        };
        let mut removed = Vec::new();
        let mut added = Vec::new();
        // A shader or node graph resynced under a material or light filter
        // is not repopulated but invalidates its representing ancestor
        // (below), as `UsdImagingStageSceneIndex::_ApplyPendingResyncs` does
        // for a `RepresentedByAncestor` prim. Any other prim under one is
        // repopulated, its subtree added though population never reached it.
        let represented: Vec<Option<(usd::Prim, usd::Prim)>> = roots
            .iter()
            .map(|root| self.representing_ancestor(root))
            .collect();
        for (root, represented) in roots.iter().zip(&represented) {
            self.forget_under(root);
            self.globals.forget_under(root);
            if represented.is_some() {
                continue;
            }
            let before: Vec<sdf::Path> = {
                let mut populated = self.populated.borrow_mut();
                let before = populated.iter().filter(|p| p.has_prefix(root)).cloned().collect();
                populated.retain(|p| !p.has_prefix(root));
                before
            };
            let now = self.populate(root);
            // As `UsdImagingStageSceneIndex::_ApplyPendingResyncs`: a prim
            // added again is resynced by its `Added` alone; each populated
            // prim that is not re-added is removed by an entry of its own.
            let again: BTreeSet<&sdf::Path> = now.iter().map(|a| &a.path).collect();
            removed.extend(
                before
                    .into_iter()
                    .filter(|p| !again.contains(p))
                    .map(|path| Removed { path }),
            );
            added.extend(now);
        }
        // Workaround, an upstream candidate in `ROADMAP.md`: when an edit
        // retires a prototype (its source instance went), openusd composes
        // the remaining instances under a new `/__Prototype_N` and reports
        // neither it nor the instances re-pointed to it. USD keeps the
        // prototype, or resyncs both. Resync them here, so that native
        // instancing, which follows notices as UsdImaging's does, sees them.
        if touches_prototype {
            let (prototypes, instances) = self.repointed_prototypes(&known_prototypes);
            for path in prototypes.into_iter().chain(instances) {
                self.forget_under(&path);
                self.globals.forget_under(&path);
                // An instance has no children here: its prim info now names
                // the new prototype.
                added.extend(self.populate(&path));
                roots.push(path);
            }
        }
        // As `UsdImagingStageSceneIndex::ApplyPendingUpdates`: the
        // properties whose spec came or went (the `Resync` pass), then those
        // whose value changed (the `Update` pass), each pass one entry per
        // prim it invalidates, so a prim both passes invalidate is dirtied
        // twice. A shader or node graph resynced under a material or light
        // filter is a property resync of it with the empty name (UsdImaging's
        // `RepresentedByAncestor` prims), replacing its other resynced
        // properties; the property changes under a repopulated prim are
        // dropped; a prim gone from the stage has no adapter, and is removed
        // only if it was added.
        let mut resynced_properties = pending.resynced_properties;
        let mut updated_properties = pending.updated_properties;
        for (root, represented) in roots.iter().zip(&represented) {
            if represented.is_some() {
                resynced_properties.insert(root.clone(), [String::new()].into());
            }
        }
        for (i, root) in roots.iter().enumerate() {
            if represented.get(i).is_some_and(Option::is_some) {
                continue;
            }
            resynced_properties.retain(|p, _| !p.has_prefix(root));
            updated_properties.retain(|p, _| !p.has_prefix(root));
        }
        let none = BTreeSet::new();
        let mut dirtied: Vec<Dirtied> = Vec::new();
        for (properties, resync) in [(resynced_properties, true), (updated_properties, false)] {
            for (prim, names) in properties {
                // A shader or node graph is represented by its material or
                // light filter, which invalidates for it.
                let (path, locators) = if let Some((ancestor, descendant)) = self.representing_ancestor(&prim)
                {
                    let locators = invalidate_from_descendant(&ancestor, &descendant, &names);
                    (ancestor.path().clone(), locators)
                } else if self.populated.borrow().contains(&prim) {
                    let locators = if resync {
                        self.invalidate(&prim, &names, &none)
                    } else {
                        self.invalidate(&prim, &none, &names)
                    };
                    (prim, locators)
                } else {
                    continue;
                };
                if !locators.is_empty() {
                    dirtied.push(Dirtied { path, locators });
                }
            }
        }
        self.observers.send_removed(self, &removed);
        self.observers.send_added(self, &added);
        self.observers.send_dirtied(self, &dirtied);
    }

    /// The USD prototypes the index has populated.
    fn populated_prototypes(&self) -> BTreeSet<sdf::Path> {
        const PREFIX: &str = "/__Prototype_";
        let Ok(first) = sdf::path(PREFIX.trim_end_matches('_')) else {
            return BTreeSet::new();
        };
        // Paths are ordered by their text: the prototypes and their
        // descendants follow `/__Prototype` together, without a walk of
        // every populated prim on each update.
        self.populated
            .borrow()
            .range(first..)
            .take_while(|p| p.as_str().starts_with("/__Prototype"))
            .filter(|p| p.is_root_prim() && p.as_str().starts_with(PREFIX))
            .cloned()
            .collect()
    }

    /// The prototypes the stage holds that the index has not populated, and
    /// the populated instances composed under one of them.
    fn repointed_prototypes(&self, known: &BTreeSet<sdf::Path>) -> (Vec<sdf::Path>, Vec<sdf::Path>) {
        let Some(stage) = self.stage() else {
            return (Vec::new(), Vec::new());
        };
        let fresh: BTreeSet<sdf::Path> = stage
            .prototypes()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !known.contains(p))
            .collect();
        if fresh.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let populated: Vec<sdf::Path> = self.populated.borrow().iter().cloned().collect();
        let instances = populated
            .into_iter()
            .filter(|path| {
                stage.prim(path).ok().is_some_and(|prim| {
                    prim.is_instance().unwrap_or(false)
                        && prim
                            .prototype()
                            .ok()
                            .flatten()
                            .is_some_and(|p| fresh.contains(&p))
                })
            })
            .collect();
        (fresh.into_iter().collect(), instances)
    }

    /// The nearest ancestor of a `Shader` or `NodeGraph` prim that
    /// represents its descendants (a material or a light filter), which
    /// represents it (`_FindResponsibleAncestor`), and the prim.
    fn representing_ancestor(&self, path: &sdf::Path) -> Option<(usd::Prim, usd::Prim)> {
        let stage = self.stage()?;
        let prim = stage.prim(path).ok()?;
        if !prim.is_valid().unwrap_or(false) {
            return None;
        }
        let ty = prim.type_name().ok().flatten()?;
        if ty != "Shader" && ty != "NodeGraph" {
            return None;
        }
        let ancestor = path
            .strict_ancestors_below_root()
            .filter_map(|a| stage.prim(&a).ok())
            .find(represents_descendants)?;
        Some((ancestor, prim))
    }

    /// The index's current time: time code 0 for a new index (a
    /// default-constructed `UsdTimeCode`, as `UsdImagingStageSceneIndex`'s),
    /// `TimeCode::EARLIEST` once a held stage was replaced, else the last
    /// time set.
    pub fn time(&self) -> TimeCode {
        self.globals.time()
    }

    /// Sets the index's time. A different time dirties every data source
    /// built so far whose value may vary over time; the same time notifies
    /// nothing.
    pub fn set_time(&self, time: TimeCode) {
        if self.globals.time() == time {
            return;
        }
        self.globals.time.set(time);
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

    /// The locators the properties `resynced` (come or gone) and `updated`
    /// (changed) on `path` dirty, from the prim's adapters.
    fn invalidate(
        &self,
        path: &sdf::Path,
        resynced: &BTreeSet<String>,
        updated: &BTreeSet<String>,
    ) -> LocatorSet {
        let mut locators = LocatorSet::new();
        let Some(stage) = self.stage() else {
            return locators;
        };
        let ctx = self.context(&stage, path);
        let Some(prim) = ctx.prim() else {
            return locators;
        };
        let usd_type = self.adapter_type(&prim);
        let apis = prim.api_schemas().unwrap_or_default();
        for adapter in self.adapters.for_prim(usd_type.as_str(), &apis) {
            for name in resynced {
                locators.insert_set(&adapter.invalidate_resynced(&ctx, name));
            }
            for name in updated {
                locators.insert_set(&adapter.invalidate(&ctx, name));
            }
        }
        locators
    }

    /// The USD type whose adapter serves `prim`: its own, or `LightFilter`
    /// for a type derived from it that has no adapter of its own
    /// (UsdImaging looks adapters up along the type's bases).
    fn adapter_type(&self, prim: &usd::Prim) -> Token {
        let usd_type = prim.type_name().ok().flatten().unwrap_or_default();
        if self.adapters.for_type(usd_type.as_str()).is_none() && material::is_light_filter(prim) {
            return Token::from("LightFilter");
        }
        usd_type
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
        // A material's descendants answer, as `UsdImagingStageSceneIndex::GetPrim`
        // does, though neither population nor `children` reaches them (the
        // material represents them, `RepresentsSelfAndDescendents`).
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
        let usd_type = self.adapter_type(&usd_prim);
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

/// Whether `prim`'s schema type is `schema` or derives from it (C++
/// `UsdPrim::IsA`), as `usd::Prim::is_a` answers, remembered per schema
/// registry, schema type and `schema`.
///
/// Workaround, an upstream candidate in `ROADMAP.md`: openusd's
/// `SchemaRegistry::is_a` walks the base graph with a fresh hash set on
/// every call, which made it most of the cost of `prim()` and `children()`.
/// The base graph is fixed once a registry is built, so the answer is too.
pub(crate) fn is_a(prim: &usd::Prim, schema: &'static str) -> bool {
    type Answers = HashMap<(Token, &'static str), bool>;
    thread_local! {
        static ANSWERS: RefCell<Option<(Arc<usd::SchemaRegistry>, Answers)>> = const { RefCell::new(None) };
    }
    let Ok(info) = prim.prim_type_info() else {
        return false;
    };
    let registry = prim.stage().schema_registry();
    ANSWERS.with_borrow_mut(|answers| {
        // Holding the registry keeps its address from being reused.
        let (_, answers) = match answers {
            Some(held) if Arc::ptr_eq(&held.0, registry) => held,
            _ => answers.insert((registry.clone(), HashMap::new())),
        };
        *answers
            .entry((info.schema_type_name().clone(), schema))
            .or_insert_with(|| registry.is_a(info.schema_type_name(), &Token::from(schema)))
    })
}

/// Whether `prim` is a `Material`.
fn is_material(prim: &usd::Prim) -> bool {
    prim.type_name().ok().flatten().is_some_and(|t| t == "Material")
}

/// Whether `prim`'s adapter represents its descendants
/// (`RepresentsSelfAndDescendents`): a material or a light filter.
fn represents_descendants(prim: &usd::Prim) -> bool {
    is_material(prim) || material::is_light_filter(prim)
}

/// What a change to `properties` of `descendant` dirties on `ancestor`,
/// which represents it (`InvalidateImagingSubprimFromDescendent`): a
/// material's terminals and nodes, or a light filter's whole `material`.
fn invalidate_from_descendant(
    ancestor: &usd::Prim,
    descendant: &usd::Prim,
    properties: &BTreeSet<String>,
) -> LocatorSet {
    if is_material(ancestor) {
        material::invalidate_from_descendant(ancestor, descendant, properties)
    } else {
        Locator::new([crate::tokens::material::MATERIAL]).into()
    }
}

/// `paths`, parents first, without the strict descendants of the prims that
/// represent them (UsdImaging prunes them from its population).
fn prune_material_descendants(stage: &Stage, paths: Vec<sdf::Path>) -> Vec<sdf::Path> {
    let mut material: Option<sdf::Path> = None;
    let mut kept = Vec::with_capacity(paths.len());
    for path in paths {
        if material
            .as_ref()
            .is_some_and(|m| path.has_prefix(m) && path != *m)
        {
            continue;
        }
        material = stage
            .prim(&path)
            .ok()
            .filter(represents_descendants)
            .map(|_| path.clone());
        kept.push(path);
    }
    kept
}

impl StageSceneIndex {
    fn compute_children(&self, stage: &Stage, path: &sdf::Path) -> Vec<sdf::Path> {
        if !path.is_abs_root() && !self.prim(path).is_defined() {
            return Vec::new();
        }
        let Ok(prim) = stage.prim(path) else {
            return Vec::new();
        };
        // A material's or light filter's descendants are not prims of the
        // index.
        if represents_descendants(&prim) {
            return Vec::new();
        }
        let mut children: Vec<sdf::Path> = prim
            .filtered_children(self.predicate())
            .map(|children| children.iter().map(|c| c.path().clone()).collect())
            .unwrap_or_default();
        if path.is_abs_root() {
            // Prototypes are prims of this index too, as in UsdImaging; the
            // native-instancing filter re-roots them.
            children.extend(stage.prototypes().unwrap_or_default());
        }
        children
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

    /// Held until a resync at or above `path` or of one of its children,
    /// as the prims are held: openusd's child listing was most of a walk's
    /// cost (an upstream candidate in `ROADMAP.md`).
    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        if let Some(children) = self.children.borrow().get(path) {
            return children.clone();
        }
        let Some(stage) = self.stage() else {
            return Vec::new();
        };
        let children = self.compute_children(&stage, path);
        self.children.borrow_mut().insert(path.clone(), children.clone());
        children
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}
