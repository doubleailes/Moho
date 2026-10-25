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
pub mod generic;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use openusd::sdf;
use openusd::tf::Token;
use openusd::usd::{self, PrimPredicate, Stage, StageBuilder, TimeCode};

use crate::data_source::{Container, DataSource};
use crate::scene_index::{Added, Observer, Observers, Prim, Removed, SceneIndex};

use adapter::{Adapter, AdapterRegistry};

/// A stage builder with openusd-schemas' registry installed, which typed
/// schemas and schema fallbacks need. Open stages for a `StageSceneIndex`
/// with it.
pub fn stage_builder() -> StageBuilder {
    Stage::builder().schema_registry(openusd_schemas::schema_registry())
}

/// State every data source of one stage scene index shares: the current
/// time.
#[derive(Default)]
pub struct Globals {
    time: Cell<Option<TimeCode>>,
}

impl Globals {
    /// The index's current time, `None` for the default time.
    pub fn time(&self) -> Option<TimeCode> {
        self.time.get()
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
}

/// One openusd stage read as a scene index.
pub struct StageSceneIndex {
    current: RefCell<Option<Current>>,
    globals: Rc<Globals>,
    adapters: AdapterRegistry,
    cache: RefCell<HashMap<sdf::Path, Prim>>,
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
            observers: Observers::default(),
        })
    }

    /// The stage being read, if any.
    pub fn stage(&self) -> Option<Stage> {
        self.current.borrow().as_ref().map(|c| c.stage.clone())
    }

    /// Replaces the stage: every prim held is removed (one removal of the
    /// absolute root), then every prim of `stage` the default traversal
    /// reaches is added.
    pub fn set_stage(&self, stage: Stage) {
        let had_stage = self.current.borrow_mut().take().is_some();
        self.cache.borrow_mut().clear();
        if had_stage {
            self.observers.send_removed(
                self,
                &[Removed {
                    path: sdf::Path::abs_root(),
                }],
            );
        }
        let mut paths = Vec::new();
        // A traversal error leaves the prims found so far: the index shows
        // what the stage could compose.
        let _ = stage.traverse(PrimPredicate::DEFAULT, |path| paths.push(path.clone()));
        *self.current.borrow_mut() = Some(Current { stage });
        let added: Vec<Added> = paths
            .into_iter()
            .map(|path| {
                let ty = self.prim(&path).ty;
                Added { path, ty }
            })
            .collect();
        self.observers.send_added(self, &added);
    }

    fn context(&self, stage: &Stage, path: &sdf::Path) -> PrimContext {
        PrimContext {
            stage: stage.clone(),
            path: path.clone(),
            globals: self.globals.clone(),
        }
    }

    /// Whether the stage exposes `path` as a prim of this index.
    fn exposes(stage: &Stage, path: &sdf::Path) -> bool {
        if !stage.prim_matches(path, PrimPredicate::DEFAULT).unwrap_or(false) {
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
        if !Self::exposes(stage, path) {
            return Prim::undefined();
        }
        let Some(usd_prim) = ctx.prim() else {
            return Prim::undefined();
        };
        let usd_type = usd_prim.type_name().ok().flatten().unwrap_or_default();
        let apis = usd_prim.api_schemas().unwrap_or_default();
        let ty = self
            .adapters
            .for_type(usd_type.as_str())
            .map(|a| a.prim_type(&ctx))
            .unwrap_or_default();
        let adapters = self.adapters.for_prim(usd_type.as_str(), &apis);
        Prim::new(ty, Rc::new(PrimContainer { ctx, adapters }))
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
        stage
            .prim(path)
            .ok()
            .and_then(|p| p.filtered_children(PrimPredicate::DEFAULT).ok())
            .map(|children| children.iter().map(|c| c.path().clone()).collect())
            .unwrap_or_default()
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.observers.add(observer);
    }
}
