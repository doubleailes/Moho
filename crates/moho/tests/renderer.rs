#![allow(clippy::unwrap_used)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::p;
use moho::chain::{self, InsertionOrder, RendererFilter, create_renderer};
use moho::filter::PrimTypePriority;
use moho::locator::{Locator, LocatorSet};
use moho::openusd::tf::Token;
use moho::openusd::usd::Stage;
use moho::prim_managing::{ManagedPrim, PrimFactory, PrimManagingObserver};
use moho::renderer::{CreateRendererError, PrimManagingRenderer, Renderer, RendererPlugin, Unsupported};
use moho::scene_index::{Added, Dirtied, RetainedSceneIndex, SceneIndexHandle};
use moho::schema::RendererCreateArgsSchema;
use moho::stage::{StageSceneIndex, stage_builder};

/// A renderer that does nothing.
struct Inert;

impl Renderer for Inert {
    fn update(&self) {}
}

/// A plugin that needs a GPU.
struct GpuOnly;

impl RendererPlugin for GpuOnly {
    type Renderer = Inert;

    fn is_supported(&self, args: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
        match args.gpu_enabled() {
            Some(true) => Ok(()),
            _ => Err(Unsupported::new("no GPU enabled")),
        }
    }

    fn create_renderer(&self, _: SceneIndexHandle, _: &RendererCreateArgsSchema) -> Option<Inert> {
        Some(Inert)
    }
}

#[test]
fn a_gpu_only_plugin_without_a_gpu() {
    let reason = GpuOnly
        .is_supported(&RendererCreateArgsSchema::build(false))
        .unwrap_err()
        .reason;
    assert!(reason.contains("GPU"), "{reason}");
    assert!(GpuOnly.is_supported(&RendererCreateArgsSchema::empty()).is_err());
    assert_eq!(
        GpuOnly.is_supported(&RendererCreateArgsSchema::build(true)),
        Ok(())
    );
}

/// The names of the filters built, in build order.
type Built = Rc<RefCell<Vec<&'static str>>>;

/// A filter recording its build, each wrapping its input in a fresh index so
/// the terminal can be told apart from the input.
fn recording(built: &Built, name: &'static str, phase: i32, order: InsertionOrder) -> RendererFilter {
    let built = built.clone();
    RendererFilter::new(phase, order, move |_input: SceneIndexHandle| {
        built.borrow_mut().push(name);
        RetainedSceneIndex::new()
    })
}

fn same(a: &SceneIndexHandle, b: &SceneIndexHandle) -> bool {
    std::ptr::addr_eq(Rc::as_ptr(a), Rc::as_ptr(b))
}

/// A plugin giving recording filters and recording the index it creates its
/// renderer over.
struct Recording {
    built: Built,
    filters: Vec<(&'static str, i32, InsertionOrder)>,
    supported: bool,
    declines: bool,
    given: RefCell<Option<SceneIndexHandle>>,
}

impl Recording {
    fn new(built: &Built, filters: Vec<(&'static str, i32, InsertionOrder)>) -> Self {
        Self {
            built: built.clone(),
            filters,
            supported: true,
            declines: false,
            given: RefCell::new(None),
        }
    }
}

impl RendererPlugin for Recording {
    type Renderer = Inert;

    fn is_supported(&self, _: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
        if self.supported {
            Ok(())
        } else {
            Err(Unsupported::new("recording says no"))
        }
    }

    fn renderer_filters(&self) -> Vec<RendererFilter> {
        self.filters
            .iter()
            .map(|&(name, phase, order)| recording(&self.built, name, phase, order))
            .collect()
    }

    fn create_renderer(&self, terminal: SceneIndexHandle, _: &RendererCreateArgsSchema) -> Option<Inert> {
        *self.given.borrow_mut() = Some(terminal);
        (!self.declines).then_some(Inert)
    }
}

#[test]
fn host_and_plugin_filters_in_one_order() {
    let built = Built::default();
    let plugin = Recording::new(
        &built,
        vec![
            ("dependency forwarding", 1000, InsertionOrder::AtEnd),
            ("light linking", 4, InsertionOrder::AtStart),
        ],
    );
    let host = [recording(&built, "scene globals", 0, InsertionOrder::AtStart)];
    let input: SceneIndexHandle = RetainedSceneIndex::new();
    let created = create_renderer(input.clone(), &host, &plugin, &RendererCreateArgsSchema::empty()).unwrap();

    assert_eq!(
        *built.borrow(),
        ["scene globals", "light linking", "dependency forwarding"]
    );
    let given = plugin.given.borrow().clone().unwrap();
    assert!(same(&given, &created.terminal));
    assert!(!same(&given, &input));
}

#[test]
fn a_host_filter_precedes_a_plugin_filter_at_equal_phase_and_order() {
    let built = Built::default();
    let plugin = Recording::new(&built, vec![("plugin", 4, InsertionOrder::AtStart)]);
    let host = [recording(&built, "host", 4, InsertionOrder::AtStart)];
    create_renderer(
        RetainedSceneIndex::new(),
        &host,
        &plugin,
        &RendererCreateArgsSchema::empty(),
    )
    .unwrap();
    assert_eq!(*built.borrow(), ["host", "plugin"]);
}

#[test]
fn an_unsupported_plugin_builds_nothing() {
    let built = Built::default();
    let mut plugin = Recording::new(&built, vec![("plugin", 4, InsertionOrder::AtStart)]);
    plugin.supported = false;
    let host = [recording(&built, "host", 0, InsertionOrder::AtStart)];
    let error = create_renderer(
        RetainedSceneIndex::new(),
        &host,
        &plugin,
        &RendererCreateArgsSchema::empty(),
    )
    .err()
    .unwrap();
    assert_eq!(
        error,
        CreateRendererError::Unsupported(Unsupported::new("recording says no"))
    );
    assert!(built.borrow().is_empty());
    assert!(plugin.given.borrow().is_none());
}

#[test]
fn a_plugin_that_declines() {
    let built = Built::default();
    let mut plugin = Recording::new(&built, Vec::new());
    plugin.declines = true;
    let error = create_renderer(
        RetainedSceneIndex::new(),
        &[],
        &plugin,
        &RendererCreateArgsSchema::empty(),
    )
    .err()
    .unwrap();
    assert_eq!(error, CreateRendererError::Declined);
    assert!(plugin.given.borrow().is_some());
}

/// Materials before everything else.
struct MaterialsFirst;

impl PrimTypePriority for MaterialsFirst {
    fn priority(&self, prim_type: &Token) -> usize {
        usize::from(prim_type != "material")
    }
    fn num_priorities(&self) -> usize {
        2
    }
}

/// What the factory and its prims saw.
#[derive(Default)]
struct Log {
    created: Vec<(String, String)>,
    dirt: Vec<(String, LocatorSet)>,
}

type SharedLog = Rc<RefCell<Log>>;

struct Managed(SharedLog);

impl ManagedPrim for Managed {
    fn dirty(&self, entry: &Dirtied, _: &PrimManagingObserver) {
        self.0
            .borrow_mut()
            .dirt
            .push((entry.path.as_str().to_owned(), entry.locators.clone()));
    }
}

/// Manages meshes and materials, declines every other type.
struct Factory(SharedLog);

impl PrimFactory for Factory {
    fn create_prim(&self, entry: &Added, _: &PrimManagingObserver) -> Option<Rc<dyn ManagedPrim>> {
        self.0
            .borrow_mut()
            .created
            .push((entry.path.as_str().to_owned(), entry.ty.as_str().to_owned()));
        (entry.ty == "mesh" || entry.ty == "material")
            .then(|| Rc::new(Managed(self.0.clone())) as Rc<dyn ManagedPrim>)
    }
}

/// A stage with a mesh defined before a material, its stage index and the
/// standard chain over it.
fn scene() -> (Stage, Rc<StageSceneIndex>, SceneIndexHandle) {
    let stage = stage_builder().in_memory("scene.usda").unwrap();
    stage.define_typed_prim("/world/geo", "Mesh").unwrap();
    stage.define_typed_prim("/Looks/red", "Material").unwrap();
    let stage_index = StageSceneIndex::new();
    stage_index.set_stage(stage.clone());
    let chain = chain::standard(stage_index.clone());
    (stage, stage_index, chain)
}

fn position(log: &SharedLog, path: &str) -> usize {
    log.borrow().created.iter().position(|(p, _)| p == path).unwrap()
}

/// Whether `log` holds `/world/geo` dirtied at exactly `mesh/doubleSided`.
fn double_sided(log: &SharedLog) -> bool {
    let double_sided = Locator::parse("mesh/doubleSided");
    log.borrow()
        .dirt
        .iter()
        .any(|(path, locators)| path == "/world/geo" && locators.iter().any(|l| *l == double_sided))
}

#[test]
fn materials_before_geometry_at_the_first_update() {
    let (_stage, _stage_index, chain) = scene();
    let log = SharedLog::default();
    let renderer = PrimManagingRenderer::new(
        chain,
        Some(Rc::new(MaterialsFirst)),
        Rc::new(Factory(log.clone())),
    );

    // The observer asks for the absolute root when it is made, as Hydra's
    // walks its index from there; the batching index holds nothing else yet.
    assert_eq!(log.borrow().created, [("/".to_owned(), String::new())]);
    assert!(renderer.observer().paths().is_empty());

    renderer.update();
    assert!(position(&log, "/Looks/red") < position(&log, "/world/geo"));
    assert!(renderer.prim(&p("/world/geo")).is_some());
    assert!(renderer.prim(&p("/Looks/red")).is_some());
}

#[test]
fn a_dirtied_entry_waits_for_the_update() {
    let (stage, stage_index, chain) = scene();
    let log = SharedLog::default();
    let renderer = PrimManagingRenderer::new(
        chain,
        Some(Rc::new(MaterialsFirst)),
        Rc::new(Factory(log.clone())),
    );
    renderer.update();

    stage
        .attribute("/world/geo.doubleSided")
        .unwrap()
        .set(true)
        .unwrap();
    stage_index.apply_pending_updates();
    assert!(log.borrow().dirt.is_empty());

    renderer.update();
    assert!(double_sided(&log));
}

#[test]
fn managed_prims_by_path() {
    let (_stage, _stage_index, chain) = scene();
    let log = SharedLog::default();
    let renderer = PrimManagingRenderer::new(chain, None, Rc::new(Factory(log.clone())));
    renderer.update();
    assert!(renderer.prim(&p("/world/geo")).is_some());
    // An Xform: asked for, declined.
    assert!(log.borrow().created.iter().any(|(path, _)| path == "/world"));
    assert!(renderer.prim(&p("/world")).is_none());
    assert!(renderer.prim(&p("/nowhere")).is_none());
}

/// A plugin whose renderer manages meshes and materials, with light linking
/// and dependency forwarding as its filters.
struct Managing(SharedLog);

impl RendererPlugin for Managing {
    type Renderer = PrimManagingRenderer;

    fn is_supported(&self, _: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
        Ok(())
    }

    fn renderer_filters(&self) -> Vec<RendererFilter> {
        vec![
            RendererFilter::light_linking(),
            RendererFilter::dependency_forwarding(),
        ]
    }

    fn create_renderer(
        &self,
        terminal: SceneIndexHandle,
        _: &RendererCreateArgsSchema,
    ) -> Option<PrimManagingRenderer> {
        Some(PrimManagingRenderer::new(
            terminal,
            Some(Rc::new(MaterialsFirst)),
            Rc::new(Factory(self.0.clone())),
        ))
    }
}

#[test]
fn an_edit_end_to_end() {
    let (stage, stage_index, chain) = scene();
    let log = SharedLog::default();
    let created = create_renderer(
        chain,
        &[RendererFilter::scene_globals(|_| {})],
        &Managing(log.clone()),
        &RendererCreateArgsSchema::empty(),
    )
    .unwrap();
    created.renderer.update();
    assert!(created.renderer.prim(&p("/world/geo")).is_some());

    stage
        .attribute("/world/geo.doubleSided")
        .unwrap()
        .set(true)
        .unwrap();
    stage_index.apply_pending_updates();
    created.renderer.update();
    assert!(double_sided(&log));
}
