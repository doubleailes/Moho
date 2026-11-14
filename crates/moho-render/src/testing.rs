//! A renderer meeting every render control requirement, whose pixels tests
//! can predict (feature `testing`).
//!
//! [`TestRenderer`] imports nothing. At each render it hashes the terminal
//! index on the owning thread (every prim's path, type and values at the
//! index's time) into a scene digest, then samples on a thread of its own:
//! each sample fills every var with bytes derived from the digest, the
//! var's AOV name and the sample count, after a configurable delay so tests
//! can stop, pause and wait mid-render. The same scene, var and sample count
//! give the same pixels; an edit to anything the index holds changes them.
//!
//! It writes products in a small raw format (`MOHORAW1`, the display window,
//! then each var's name, format and pixels) at each product's name, so tests
//! can check a file without an image library.
//!
//! An edit is classified from its dirty locators: a namespaced render
//! setting costs `Continue` (or `Restart` without
//! [`TestConfig::continue_on_edit`]), one of the
//! [`TestConfig::create_only_settings`] costs `Recreate`, anything else
//! `Restart`.
//!
//! # The host loop
//!
//! ```
//! use std::rc::Rc;
//! use std::time::Duration;
//!
//! use moho::chain::{self, RendererFilter, create_renderer};
//! use moho::locator::Locator;
//! use moho::openusd::sdf;
//! use moho::scene_index::testing::{Notice, RecordingObserver};
//! use moho::renderer::Renderer;
//! use moho::schema::RendererCreateArgsSchema;
//! use moho::stage::{StageSceneIndex, stage_builder};
//! use moho_render::testing::{TestConfig, TestPlugin};
//! use moho_render::{Convergence, ControlledRenderer, RenderRequest, RunState};
//!
//! // A stage, its index, and the host's scene globals naming a camera.
//! let stage = stage_builder().in_memory("scene.usda").unwrap();
//! stage.define_typed_prim("/cam", "Camera").unwrap();
//! let stage_index = StageSceneIndex::new();
//! stage_index.set_stage(stage.clone());
//! let host = [RendererFilter::scene_globals(|globals| {
//!     globals.set_primary_camera_prim_path(sdf::path("/cam").unwrap());
//! })];
//!
//! let plugin = TestPlugin::new(TestConfig::default());
//! let created = create_renderer(
//!     chain::standard(stage_index.clone()),
//!     &host,
//!     &plugin,
//!     &RendererCreateArgsSchema::empty(),
//! )
//! .unwrap();
//! let renderer = created.renderer;
//!
//! // An interactive host observes the output index and reads a frame when
//! // its var's generation is dirtied.
//! let output = renderer.output();
//! let recorder = RecordingObserver::on(&*output);
//! renderer.update();
//! renderer
//!     .render(&RenderRequest {
//!         until: Convergence::Samples(4),
//!         ..RenderRequest::default()
//!     })
//!     .unwrap();
//!
//! // A batch host waits for convergence on any thread...
//! let control = renderer.control();
//! let mut state = control.state();
//! while state.is_active() {
//!     state = control.wait(&state, Duration::from_secs(10));
//! }
//! assert_eq!(state, RunState::Converged { samples: 4 });
//!
//! // ...and the owning thread sends the output notices at its next tick.
//! renderer.update();
//! let var = sdf::path("/__moho_render__/product/color").unwrap();
//! let generation = Locator::parse("renderBuffer/generation");
//! assert!(recorder.take().iter().any(|n| matches!(
//!     n,
//!     Notice::Dirtied(d) if d.path == var && d.locators.contains(&generation)
//! )));
//! let frame = control.frame(&var).unwrap();
//! assert_eq!(frame.samples, 4);
//! ```

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use moho::data_source::{ContainerHandle, DataSource};
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::renderer::{Renderer, RendererPlugin, Unsupported};
use moho::scene_index::{SceneIndex, SceneIndexHandle};
use moho::schema::RendererCreateArgsSchema;
use moho::tokens::render as r;

use crate::capabilities::{AovDescriptor, Capabilities, CommandDescriptor, SettingDescriptor, SettingType};
use crate::control::{Control, ControlHandle, Shared};
use crate::output::OutputSceneIndex;
use crate::renderer::{ControlledRenderer, InvalidationTracker, TrackedNotice};
use crate::targets::RenderTargets;
use crate::types::{
    Convergence, Frame, Invalidation, PixelFormat, PixelRect, ProductWriter, RenderError, RenderRequest,
    RunState,
};

/// How a [`TestRenderer`] behaves.
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// The time each sample takes.
    pub sample_delay: Duration,
    /// The samples after which [`Convergence::Converged`] is met.
    pub converge_at: u32,
    /// The image size without render settings.
    pub default_resolution: (u32, u32),
    pub pause: bool,
    pub stop: bool,
    pub continue_on_edit: bool,
    pub writes_products: bool,
    /// Settings whose change needs the renderer recreated.
    pub create_only_settings: Vec<Token>,
    /// Fails the render when this sample is reached.
    pub fail_at_sample: Option<u32>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            sample_delay: Duration::from_millis(1),
            converge_at: 8,
            default_resolution: (8, 4),
            pause: true,
            stop: true,
            continue_on_edit: true,
            writes_products: true,
            create_only_settings: vec![Token::new("test:integrator")],
            fail_at_sample: None,
        }
    }
}

/// The test renderer's plugin.
pub struct TestPlugin {
    pub config: TestConfig,
}

impl TestPlugin {
    pub fn new(config: TestConfig) -> Self {
        Self { config }
    }
}

impl RendererPlugin for TestPlugin {
    type Renderer = TestRenderer;

    fn is_supported(&self, _: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
        Ok(())
    }

    fn create_renderer(
        &self,
        terminal: SceneIndexHandle,
        _: &RendererCreateArgsSchema,
    ) -> Option<TestRenderer> {
        Some(TestRenderer::new(terminal, self.config.clone()))
    }
}

/// The name of the one command the test renderer declares.
pub const FLUSH_TEXTURE_CACHE: &str = "flushTextureCache";

/// The test renderer's settings and command.
fn capabilities(config: &TestConfig) -> Capabilities {
    Capabilities {
        pause: config.pause,
        stop: config.stop,
        progressive: true,
        continue_on_edit: config.continue_on_edit,
        writes_products: config.writes_products,
        material_contexts: vec![Token::new("")],
        aovs: vec![AovDescriptor {
            name: Token::new("color"),
            format: PixelFormat::F32x4,
            clear_value: Value::Float(0.0),
            multi_sampled: false,
        }],
        settings: vec![
            SettingDescriptor {
                key: Token::new("test:quality"),
                ty: SettingType::Int,
                default: Value::Int(64),
                range: Some((Value::Int(1), Value::Int(4096))),
            },
            SettingDescriptor {
                key: Token::new("test:integrator"),
                ty: SettingType::Enum(vec![Token::new("pt"), Token::new("bdpt")]),
                default: Value::Token(Token::new("pt")),
                range: None,
            },
        ],
        commands: vec![CommandDescriptor {
            name: Token::new(FLUSH_TEXTURE_CACHE),
            description: "Does nothing: the test renderer has no textures.".to_string(),
            arguments: Vec::new(),
        }],
    }
}

/// What an edit costs the test renderer.
fn classify(notice: &TrackedNotice, continue_on_edit: bool, create_only: &[Token]) -> Invalidation {
    let TrackedNotice::Dirtied(dirtied) = notice else {
        return Invalidation::Restart;
    };
    dirtied
        .locators
        .iter()
        .map(|locator| match locator.elements() {
            [settings, namespaced, key]
                if settings.as_str() == r::RENDER_SETTINGS
                    && namespaced.as_str() == r::NAMESPACED_SETTINGS =>
            {
                if create_only.contains(key) {
                    Invalidation::Recreate
                } else if continue_on_edit {
                    Invalidation::Continue
                } else {
                    Invalidation::Restart
                }
            }
            _ => Invalidation::Restart,
        })
        .max()
        .unwrap_or(Invalidation::Restart)
}

/// A var as the sampling thread renders it.
#[derive(Debug, Clone, PartialEq)]
struct JobVar {
    path: sdf::Path,
    name: Token,
    format: PixelFormat,
    display_window: PixelRect,
    data_window: PixelRect,
}

/// A product as the sampling thread writes it.
#[derive(Debug, Clone)]
struct JobProduct {
    path: sdf::Path,
    name: Option<Token>,
    display_window: PixelRect,
    vars: Vec<sdf::Path>,
}

/// Everything the sampling thread needs: plain values that cross threads.
#[derive(Debug, Clone)]
struct Job {
    vars: Vec<JobVar>,
    products: Vec<JobProduct>,
    until: Convergence,
    writer: ProductWriter,
    digest: u64,
    converge_at: u32,
    delay: Duration,
    fail_at: Option<u32>,
}

impl Job {
    fn new(targets: &RenderTargets, request: &RenderRequest, digest: u64, config: &TestConfig) -> Self {
        let mut vars: Vec<JobVar> = Vec::new();
        for product in &targets.products {
            for var in &product.vars {
                if !vars.iter().any(|v| v.path == var.path) {
                    vars.push(JobVar {
                        path: var.path.clone(),
                        name: var.name.clone(),
                        format: var.format,
                        display_window: product.display_window,
                        data_window: product.data_window,
                    });
                }
            }
        }
        let products = targets
            .products
            .iter()
            .map(|p| JobProduct {
                path: p.path.clone(),
                name: p.name.clone(),
                display_window: p.display_window,
                vars: p.vars.iter().map(|v| v.path.clone()).collect(),
            })
            .collect();
        Self {
            vars,
            products,
            until: request.until,
            writer: request.writer,
            digest,
            converge_at: config.converge_at,
            delay: config.sample_delay,
            fail_at: config.fail_at_sample,
        }
    }

    fn reached(&self, samples: u32, started: Instant) -> bool {
        match self.until {
            Convergence::Converged => samples >= self.converge_at,
            Convergence::Samples(n) => samples >= n,
            Convergence::Seconds(seconds) => started.elapsed().as_secs_f32() >= seconds,
            Convergence::Never => false,
        }
    }

    fn progress(&self, samples: u32, started: Instant) -> Option<f32> {
        let fraction = |done: f32, total: f32| (total > 0.0).then(|| (done / total).min(1.0));
        match self.until {
            Convergence::Converged => fraction(samples as f32, self.converge_at as f32),
            Convergence::Samples(n) => fraction(samples as f32, n as f32),
            Convergence::Seconds(seconds) => fraction(started.elapsed().as_secs_f32(), seconds),
            Convergence::Never => None,
        }
    }
}

/// The pixels of `var` at `samples` over a scene with `digest`.
pub fn pixels(digest: u64, aov: &str, samples: u32, bytes: usize) -> Arc<[u8]> {
    let mut hasher = DefaultHasher::new();
    (digest, aov, samples).hash(&mut hasher);
    let word = hasher.finish().to_le_bytes();
    word.iter().copied().cycle().take(bytes).collect()
}

/// The sampling thread.
fn sample(job: Job, shared: Arc<Shared>, epoch: u64, start: u32) {
    let started = Instant::now();
    let mut samples = start;
    loop {
        if !shared.wait_while_paused(epoch) {
            return;
        }
        if samples > 0 && job.reached(samples, started) {
            break;
        }
        thread::sleep(job.delay);
        samples += 1;
        if job.fail_at == Some(samples) {
            shared.set_state(
                epoch,
                RunState::Failed {
                    reason: format!("test failure at sample {samples}").into(),
                },
            );
            return;
        }
        for var in &job.vars {
            let frame = Frame {
                generation: 0,
                samples,
                format: var.format,
                display_window: var.display_window,
                data_window: var.data_window,
                pixels: pixels(
                    job.digest,
                    var.name.as_str(),
                    samples,
                    var.data_window.area() * var.format.bytes_per_pixel(),
                ),
            };
            shared.publish_frame(epoch, &var.path, frame);
        }
        shared.set_stat("samples", Value::Int(samples as i32));
        shared.set_state(
            epoch,
            RunState::Rendering {
                samples,
                progress: job.progress(samples, started),
            },
        );
    }
    if job.writer == ProductWriter::Renderer {
        for product in &job.products {
            if let Err(reason) = write_product(&shared, product) {
                shared.set_state(
                    epoch,
                    RunState::Failed {
                        reason: reason.into(),
                    },
                );
                return;
            }
            if let Some(name) = &product.name {
                shared.record_written(epoch, &product.path, name.as_str());
            }
        }
    }
    shared.set_state(epoch, RunState::Converged { samples });
}

/// Writes `product` in the raw format at its name.
fn write_product(shared: &Shared, product: &JobProduct) -> Result<(), String> {
    let Some(name) = &product.name else {
        return Ok(());
    };
    let mut bytes = b"MOHORAW1".to_vec();
    bytes.extend(product.display_window.width.to_le_bytes());
    bytes.extend(product.display_window.height.to_le_bytes());
    bytes.extend((product.vars.len() as u32).to_le_bytes());
    for var in &product.vars {
        let frame = shared.frame(var);
        let var_name = var.name().unwrap_or_default();
        bytes.extend((var_name.len() as u32).to_le_bytes());
        bytes.extend(var_name.as_bytes());
        let format = frame.as_ref().map(|f| f.format.token()).unwrap_or_default();
        bytes.extend((format.as_str().len() as u32).to_le_bytes());
        bytes.extend(format.as_str().as_bytes());
        let pixels = frame.map(|f| f.pixels.to_vec()).unwrap_or_default();
        bytes.extend((pixels.len() as u64).to_le_bytes());
        bytes.extend(pixels);
    }
    std::fs::write(name.as_str(), bytes).map_err(|e| format!("cannot write {}: {e}", name.as_str()))
}

/// A hash of everything `index` holds: every prim's path, type and values at
/// the index's time.
pub fn scene_digest(index: &dyn SceneIndex) -> u64 {
    fn data(source: &DataSource, hasher: &mut DefaultHasher) {
        match source {
            DataSource::Container(container) => container_data(container, hasher),
            DataSource::Vector(vector) => {
                vector.len().hash(hasher);
                for i in 0..vector.len() {
                    if let Some(element) = vector.get(i) {
                        data(&element, hasher);
                    }
                }
            }
            DataSource::Sampled(sampled) => format!("{:?}", sampled.value(0.0)).hash(hasher),
            DataSource::Block => 0u8.hash(hasher),
        }
    }
    fn container_data(container: &ContainerHandle, hasher: &mut DefaultHasher) {
        for name in container.names() {
            name.as_str().hash(hasher);
            if let Some(child) = container.get(&name) {
                data(&child, hasher);
            }
        }
    }
    let mut hasher = DefaultHasher::new();
    let mut stack = vec![sdf::Path::abs_root()];
    while let Some(path) = stack.pop() {
        let prim = index.prim(&path);
        path.as_str().hash(&mut hasher);
        prim.ty.as_str().hash(&mut hasher);
        if let Some(container) = &prim.data {
            container_data(container, &mut hasher);
        }
        stack.extend(index.children(&path).into_iter().rev());
    }
    hasher.finish()
}

/// The test renderer.
pub struct TestRenderer {
    terminal: SceneIndexHandle,
    config: TestConfig,
    capabilities: Capabilities,
    tracker: InvalidationTracker,
    shared: Arc<Shared>,
    output: Rc<OutputSceneIndex>,
    worker: RefCell<Option<JoinHandle<()>>>,
    /// The vars of the render in flight, to tell whether a new render can
    /// keep its samples.
    layout: RefCell<Vec<JobVar>>,
}

impl TestRenderer {
    pub fn new(terminal: SceneIndexHandle, config: TestConfig) -> Self {
        let capabilities = capabilities(&config);
        let continue_on_edit = config.continue_on_edit;
        let create_only = config.create_only_settings.clone();
        let tracker = InvalidationTracker::new(terminal.clone(), None, None, move |notice| {
            classify(notice, continue_on_edit, &create_only)
        });
        let shared = Shared::new(config.pause, config.stop);
        let output = OutputSceneIndex::new(&capabilities);
        Self {
            terminal,
            config,
            capabilities,
            tracker,
            shared,
            output,
            worker: RefCell::default(),
            layout: RefCell::default(),
        }
    }

    /// The scene digest the render in flight, or the next, is made from.
    pub fn digest(&self) -> u64 {
        scene_digest(&*self.terminal)
    }

    /// Ends the sampling thread, if any.
    fn retire_worker(&self) {
        self.shared.retire();
        if let Some(worker) = self.worker.borrow_mut().take() {
            // A sampling thread that panicked has nothing left to retire.
            let _ = worker.join();
        }
    }
}

impl Renderer for TestRenderer {
    fn update(&self) {
        self.tracker.update();
        self.poll();
    }
}

impl ControlledRenderer for TestRenderer {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    fn invalidation(&self) -> Invalidation {
        self.tracker.invalidation()
    }

    fn render(&self, request: &RenderRequest) -> Result<(), RenderError> {
        let invalidation = self.tracker.invalidation();
        if invalidation == Invalidation::Recreate {
            return Err(RenderError::RecreateRequired);
        }
        if request.writer == ProductWriter::Renderer && !self.capabilities.writes_products {
            return Err(RenderError::Unsupported("product writing"));
        }
        let default_aov = self
            .capabilities
            .default_aov()
            .ok_or_else(|| RenderError::Failed("no default AOV".to_string()))?;
        let targets = RenderTargets::resolve(
            &*self.terminal,
            request,
            default_aov,
            self.config.default_resolution,
        )?;
        let job = Job::new(&targets, request, self.digest(), &self.config);

        let state = self.shared.state();
        let keep = state.is_active()
            && invalidation <= Invalidation::Continue
            && self.capabilities.continue_on_edit
            && *self.layout.borrow() == job.vars;
        let start = if keep {
            state.samples().unwrap_or_default()
        } else {
            0
        };

        self.retire_worker();
        let epoch = self.shared.begin(job.vars.iter().map(|v| v.path.clone()));
        let aovs = self.capabilities.aovs.clone();
        self.output.set_targets(&targets, |name| {
            aovs.iter().any(|a| a.name == *name && a.multi_sampled)
        });
        self.tracker.reset();
        *self.layout.borrow_mut() = job.vars.clone();

        let shared = self.shared.clone();
        let worker = thread::Builder::new()
            .name("moho-render-test".to_string())
            .spawn(move || sample(job, shared, epoch, start))
            .map_err(|e| RenderError::Failed(format!("cannot start the sampling thread: {e}")))?;
        *self.worker.borrow_mut() = Some(worker);
        Ok(())
    }

    fn output(&self) -> SceneIndexHandle {
        self.output.clone()
    }

    fn control(&self) -> ControlHandle {
        ControlHandle::new(self.shared.clone())
    }

    fn poll(&self) {
        self.output.poll(&self.shared);
    }

    fn invoke(&self, command: &Token, _args: Option<ContainerHandle>) -> Result<(), RenderError> {
        match self.capabilities.command(command) {
            Some(_) => Ok(()),
            None => Err(RenderError::UnknownCommand(command.clone())),
        }
    }
}

impl Drop for TestRenderer {
    fn drop(&mut self) {
        self.retire_worker();
    }
}
