//! The output scene index: what a renderer reports, observed like any input
//! index.
//!
//! - The absolute root holds `renderer`: `state` (the run state's kind),
//!   `samples`, `progress` (absent while unknown), `failure` (in the
//!   `failed` state), `stats` (an open container each renderer fills) and
//!   `capabilities` ([`Capabilities::to_container`]).
//! - Each rendered product has a prim of type `renderProductOutput` at the
//!   product's path, with `renderProductOutput/product` and, once the
//!   renderer wrote it, `renderProductOutput/written`.
//! - Each rendered var has a prim of type `renderBuffer` at the var's path,
//!   with `renderBuffer/format`, `dimensions`, `multiSampled`, `generation`
//!   and `converged`, named after `HdRenderBufferSchema`.
//!
//! It holds no pixels: a var's `renderBuffer/generation` is dirtied once per
//! [`poll`](OutputSceneIndex::poll) that finds a new snapshot, and the host
//! reads the frame through the control handle. Notices are sent only from
//! `poll` and [`set_targets`](OutputSceneIndex::set_targets), on the thread
//! that owns the index.
//!
//! Products and vars are keyed by their stage paths. A var two products
//! share is one prim, rendered once.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use moho::data_source::retained::RetainedContainer;
use moho::data_source::{Container, ContainerHandle, DataSource};
use moho::locator::{Locator, LocatorSet};
use moho::openusd::gf;
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::scene_index::{Observer, Prim, RetainedSceneIndex, SceneIndex};

use crate::capabilities::{Capabilities, value};
use crate::control::Shared;
use crate::targets::RenderTargets;
use crate::tokens::{render_buffer as b, render_product_output as o, renderer as r};
use crate::types::{PixelFormat, RunState};

/// What a renderer reports, as a scene index.
pub struct OutputSceneIndex {
    retained: Rc<RetainedSceneIndex>,
    state: Rc<RefCell<OutputState>>,
}

struct OutputState {
    run: RunState,
    stats: BTreeMap<Token, Value>,
    capabilities: ContainerHandle,
    products: BTreeMap<sdf::Path, ProductOut>,
    vars: BTreeMap<sdf::Path, VarOut>,
}

struct ProductOut {
    written: Option<String>,
}

#[derive(Clone, PartialEq)]
struct VarOut {
    format: PixelFormat,
    dimensions: (u32, u32),
    multi_sampled: bool,
    generation: u64,
    converged: bool,
}

/// Which prim a live container answers for.
#[derive(Clone)]
enum Kind {
    Root,
    Product(sdf::Path),
    Var(sdf::Path),
}

/// A prim's data, read from the output state when asked, so a poll changes
/// it in place.
struct Live {
    state: Rc<RefCell<OutputState>>,
    kind: Kind,
}

impl Live {
    fn snapshot(&self) -> RetainedContainer {
        let state = self.state.borrow();
        match &self.kind {
            Kind::Root => {
                let run = &state.run;
                let mut renderer = RetainedContainer::new([
                    (r::STATE, value(run.token())),
                    (r::SAMPLES, value(run.samples().unwrap_or_default() as i32)),
                    (
                        r::STATS,
                        RetainedContainer::new(
                            state.stats.iter().map(|(k, v)| (k.clone(), value(v.clone()))),
                        )
                        .into_data_source(),
                    ),
                    (r::CAPABILITIES, DataSource::Container(state.capabilities.clone())),
                ]);
                if let Some(progress) = run.progress() {
                    renderer.set(r::PROGRESS, value(progress));
                }
                if let RunState::Failed { reason } = run {
                    renderer.set(r::FAILURE, value(reason.to_string()));
                }
                RetainedContainer::new([(r::RENDERER, renderer.into_data_source())])
            }
            Kind::Product(path) => {
                let mut product = RetainedContainer::new([(o::PRODUCT, value(Value::Path(path.clone())))]);
                if let Some(written) = state.products.get(path).and_then(|p| p.written.clone()) {
                    product.set(o::WRITTEN, value(Value::AssetPath(written.as_str().into())));
                }
                RetainedContainer::new([(o::RENDER_PRODUCT_OUTPUT, product.into_data_source())])
            }
            Kind::Var(path) => {
                let Some(var) = state.vars.get(path) else {
                    return RetainedContainer::default();
                };
                let buffer = RetainedContainer::new([
                    (b::FORMAT, value(var.format.token())),
                    (
                        b::DIMENSIONS,
                        value(gf::Vec2i {
                            x: var.dimensions.0 as i32,
                            y: var.dimensions.1 as i32,
                        }),
                    ),
                    (b::MULTI_SAMPLED, value(var.multi_sampled)),
                    (b::GENERATION, value(var.generation as i64)),
                    (b::CONVERGED, value(var.converged)),
                ]);
                RetainedContainer::new([(b::RENDER_BUFFER, buffer.into_data_source())])
            }
        }
    }
}

impl Container for Live {
    fn names(&self) -> Vec<Token> {
        self.snapshot().names()
    }

    fn get(&self, name: &Token) -> Option<DataSource> {
        self.snapshot().get(name)
    }
}

fn locator(container: &str, field: &str) -> Locator {
    Locator::new([container, field])
}

impl OutputSceneIndex {
    /// An output index for a renderer with `capabilities`, idle, with no
    /// product.
    pub fn new(capabilities: &Capabilities) -> Rc<Self> {
        let state = Rc::new(RefCell::new(OutputState {
            run: RunState::Idle,
            stats: BTreeMap::new(),
            capabilities: capabilities.to_container().into_handle(),
            products: BTreeMap::new(),
            vars: BTreeMap::new(),
        }));
        let retained = RetainedSceneIndex::new();
        retained.add_prims([(sdf::Path::abs_root(), Self::prim(&state, "", Kind::Root))]);
        Rc::new(Self { retained, state })
    }

    fn prim(state: &Rc<RefCell<OutputState>>, ty: &'static str, kind: Kind) -> Prim {
        Prim::new(
            Token::new(ty),
            Rc::new(Live {
                state: state.clone(),
                kind,
            }),
        )
    }

    /// Makes the products and vars of `targets` the rendered ones: adds the
    /// new prims, removes those no longer rendered, and dirties a kept var's
    /// format and dimensions when they changed. A kept var keeps its
    /// generation; every var becomes unconverged. `multi_sampled` answers
    /// for each var.
    pub fn set_targets(&self, targets: &RenderTargets, multi_sampled: impl Fn(&Token) -> bool) {
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut dirtied = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            let products: Vec<sdf::Path> = targets.products.iter().map(|p| p.path.clone()).collect();
            let mut vars = BTreeMap::new();
            for product in &targets.products {
                for var in &product.vars {
                    vars.entry(var.path.clone()).or_insert(VarOut {
                        format: var.format,
                        dimensions: (product.data_window.width, product.data_window.height),
                        multi_sampled: multi_sampled(&var.name),
                        generation: 0,
                        converged: false,
                    });
                }
            }

            let gone: Vec<sdf::Path> = state
                .products
                .keys()
                .filter(|p| !products.contains(p))
                .cloned()
                .collect();
            for path in gone {
                state.products.remove(&path);
                removed.push(path);
            }
            for path in products {
                if !state.products.contains_key(&path) {
                    state.products.insert(path.clone(), ProductOut { written: None });
                    added.push((
                        path.clone(),
                        Self::prim(&self.state, o::RENDER_PRODUCT_OUTPUT, Kind::Product(path)),
                    ));
                } else if let Some(product) = state.products.get_mut(&path) {
                    product.written = None;
                }
            }

            let gone: Vec<sdf::Path> = state
                .vars
                .keys()
                .filter(|v| !vars.contains_key(*v))
                .cloned()
                .collect();
            for path in gone {
                state.vars.remove(&path);
                removed.push(path);
            }
            for (path, mut new) in vars {
                match state.vars.get(&path) {
                    Some(old) => {
                        new.generation = old.generation;
                        let mut locators = LocatorSet::new();
                        if old.format != new.format {
                            locators.insert(locator(b::RENDER_BUFFER, b::FORMAT));
                        }
                        if old.dimensions != new.dimensions {
                            locators.insert(locator(b::RENDER_BUFFER, b::DIMENSIONS));
                        }
                        if old.multi_sampled != new.multi_sampled {
                            locators.insert(locator(b::RENDER_BUFFER, b::MULTI_SAMPLED));
                        }
                        if old.converged {
                            locators.insert(locator(b::RENDER_BUFFER, b::CONVERGED));
                        }
                        if !locators.is_empty() {
                            dirtied.push((path.clone(), locators));
                        }
                    }
                    None => {
                        added.push((
                            path.clone(),
                            Self::prim(&self.state, b::RENDER_BUFFER, Kind::Var(path.clone())),
                        ));
                    }
                }
                state.vars.insert(path, new);
            }
        }
        if !removed.is_empty() {
            self.retained.remove_prims(removed);
        }
        if !added.is_empty() {
            self.retained.add_prims(added);
        }
        if !dirtied.is_empty() {
            self.retained.dirty_prims(dirtied);
        }
    }

    /// Takes what changed in `shared` and sends it as one `dirtied` batch:
    /// each var with a new snapshot at `renderBuffer/generation` (once,
    /// however many snapshots), the root's `renderer/state` when the state's
    /// kind changed (with `samples`, `progress` and `failure` when they
    /// did), a product written at `renderProductOutput/written`, and the
    /// stats. Converging marks every var converged; a new render clears it.
    pub fn poll(&self, shared: &Shared) {
        let changes = shared.take_changes();
        let mut dirtied: Vec<(sdf::Path, LocatorSet)> = Vec::new();
        {
            let mut state = self.state.borrow_mut();
            let mut root = LocatorSet::new();
            if let Some(new) = changes.state {
                let old = std::mem::replace(&mut state.run, new.clone());
                if !old.same_kind(&new) {
                    root.insert(locator(r::RENDERER, r::STATE));
                }
                if old.samples() != new.samples() {
                    root.insert(locator(r::RENDERER, r::SAMPLES));
                }
                if old.progress() != new.progress() {
                    root.insert(locator(r::RENDERER, r::PROGRESS));
                }
                if matches!(old, RunState::Failed { .. }) != matches!(new, RunState::Failed { .. })
                    || (matches!(new, RunState::Failed { .. }) && old != new)
                {
                    root.insert(locator(r::RENDERER, r::FAILURE));
                }
                let converged = matches!(new, RunState::Converged { .. });
                for (path, var) in state.vars.iter_mut() {
                    if var.converged != converged {
                        var.converged = converged;
                        dirtied.push((
                            path.clone(),
                            LocatorSet::from(locator(b::RENDER_BUFFER, b::CONVERGED)),
                        ));
                    }
                }
            }
            if let Some(stats) = changes.stats {
                state.stats = stats;
                root.insert(locator(r::RENDERER, r::STATS));
            }
            if !root.is_empty() {
                dirtied.push((sdf::Path::abs_root(), root));
            }
            for (path, frame) in changes.frames {
                let Some(var) = state.vars.get_mut(&path) else {
                    continue;
                };
                let mut locators = LocatorSet::from(locator(b::RENDER_BUFFER, b::GENERATION));
                var.generation = frame.generation;
                let dimensions = (frame.data_window.width, frame.data_window.height);
                if var.dimensions != dimensions {
                    var.dimensions = dimensions;
                    locators.insert(locator(b::RENDER_BUFFER, b::DIMENSIONS));
                }
                if var.format != frame.format {
                    var.format = frame.format;
                    locators.insert(locator(b::RENDER_BUFFER, b::FORMAT));
                }
                match dirtied.iter_mut().find(|(p, _)| *p == path) {
                    Some((_, set)) => set.insert_set(&locators),
                    None => dirtied.push((path, locators)),
                }
            }
            for (path, file) in changes.written {
                if let Some(product) = state.products.get_mut(&path) {
                    product.written = Some(file);
                    dirtied.push((
                        path,
                        LocatorSet::from(locator(o::RENDER_PRODUCT_OUTPUT, o::WRITTEN)),
                    ));
                }
            }
        }
        if !dirtied.is_empty() {
            self.retained.dirty_prims(dirtied);
        }
    }

    /// The run state the index last reported.
    pub fn reported_state(&self) -> RunState {
        self.state.borrow().run.clone()
    }
}

impl SceneIndex for OutputSceneIndex {
    fn prim(&self, path: &sdf::Path) -> Prim {
        self.retained.prim(path)
    }

    fn children(&self, path: &sdf::Path) -> Vec<sdf::Path> {
        self.retained.children(path)
    }

    fn add_observer(&self, observer: Weak<dyn Observer>) {
        self.retained.add_observer(observer);
    }
}
