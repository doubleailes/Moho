//! Render control end to end, through `chain::create_renderer` and the test
//! renderer: one test per scenario of the `render-control` spec.

#![allow(clippy::unwrap_used)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use common::{CAMERA_SCENE, SETTINGS_SCENE, Scene, p};
use moho::chain::create_renderer;
use moho::data_source::locate;
use moho::locator::Locator;
use moho::openusd::sdf;
use moho::openusd::tf::Token;
use moho::openusd::vt::Value;
use moho::renderer::Renderer;
use moho::schema::RendererCreateArgsSchema;
use moho_render::testing::{TestConfig, TestPlugin, TestRenderer};
use moho_render::{
    ControlHandle, ControlledRenderer, Convergence, Invalidation, PixelRect, ProductWriter, RenderError,
    RenderRequest, RunState, SettingType, SettingsOverrideSceneIndex,
};

const TIMEOUT: Duration = Duration::from_secs(20);

struct Setup {
    scene: Scene,
    renderer: TestRenderer,
    overrides: Rc<SettingsOverrideSceneIndex>,
}

fn setup(text: &str, config: TestConfig) -> Setup {
    let scene = Scene::new(text);
    let slot: Rc<RefCell<Option<Rc<SettingsOverrideSceneIndex>>>> = Rc::default();
    let host = {
        let slot = slot.clone();
        vec![SettingsOverrideSceneIndex::filter(move |index| {
            *slot.borrow_mut() = Some(index);
        })]
    };
    let created = create_renderer(
        scene.terminal.clone(),
        &host,
        &TestPlugin::new(config),
        &RendererCreateArgsSchema::empty(),
    )
    .unwrap();
    let overrides = slot.borrow().clone().unwrap();
    let renderer = created.renderer;
    renderer.update();
    Setup {
        scene,
        renderer,
        overrides,
    }
}

fn slow() -> TestConfig {
    TestConfig {
        sample_delay: Duration::from_millis(20),
        ..TestConfig::default()
    }
}

fn request(until: Convergence) -> RenderRequest {
    RenderRequest {
        until,
        ..RenderRequest::default()
    }
}

/// Waits until `done` holds for the state, failing past the timeout.
fn wait_for(control: &ControlHandle, done: impl Fn(&RunState) -> bool) -> RunState {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let state = control.state();
        if done(&state) {
            return state;
        }
        assert!(Instant::now() < deadline, "timed out in {state:?}");
        thread::sleep(Duration::from_millis(1));
    }
}

fn samples_at_least(n: u32) -> impl Fn(&RunState) -> bool {
    move |s| s.samples().is_some_and(|done| done >= n) && matches!(s, RunState::Rendering { .. })
}

/// Waits until `var` has a frame newer than `generation`.
fn next_frame(setup: &Setup, var: &sdf::Path, generation: u64) -> moho_render::Frame {
    let control = setup.renderer.control();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(frame) = control.frame(var)
            && frame.generation > generation
        {
            return (*frame).clone();
        }
        assert!(Instant::now() < deadline, "no frame newer than {generation}");
        thread::sleep(Duration::from_millis(1));
    }
}

fn converged(setup: &Setup) -> RunState {
    let control = setup.renderer.control();
    wait_for(&control, |s| !s.is_active())
}

fn color() -> sdf::Path {
    p("/Render/color")
}

#[test]
fn capabilities_before_rendering() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    let caps = s.renderer.capabilities();
    assert!(caps.pause && caps.stop && caps.progressive && caps.continue_on_edit && caps.writes_products);
    let quality = caps
        .settings
        .iter()
        .find(|d| d.key.as_str() == "test:quality")
        .unwrap();
    assert_eq!(quality.ty, SettingType::Int);
    assert_eq!(quality.default, Value::Int(64));
    assert_eq!(s.renderer.control().state(), RunState::Idle);
}

/// The settings' display names, help and groups, as the output index gives
/// them to a settings panel.
#[test]
fn settings_are_described_in_the_output_index() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    let output = s.renderer.output();
    let root = output.prim(&sdf::Path::abs_root()).data.unwrap();
    let settings = locate(&root, &Locator::parse("renderer/capabilities/settings")).unwrap();
    let settings = settings.as_vector().unwrap();
    let quality = (0..settings.len())
        .filter_map(|i| settings.get(i)?.as_container().cloned())
        .find(|c| {
            locate(c, &Locator::parse("key")).and_then(|d| d.value())
                == Some(Value::Token(Token::new("test:quality")))
        })
        .unwrap();
    let read = |name: &str| locate(&quality, &Locator::parse(name)).and_then(|d| d.value());
    assert_eq!(read("label"), Some(Value::String("Quality".into())));
    assert_eq!(read("group"), Some(Value::String("Sampling".into())));
    assert!(matches!(read("help"), Some(Value::String(help)) if !help.is_empty()));
}

#[test]
fn render_returns_at_once() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let state = s.renderer.control().state();
    assert!(
        matches!(
            state,
            RunState::Loading | RunState::Rendering { samples: 0..=1, .. }
        ),
        "{state:?}"
    );
    s.renderer.control().stop();
}

#[test]
fn convergence_at_four_samples() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer.render(&request(Convergence::Samples(4))).unwrap();
    assert_eq!(converged(&s), RunState::Converged { samples: 4 });
    let frame = s.renderer.control().frame(&color()).unwrap();
    assert_eq!(frame.samples, 4);
    thread::sleep(Duration::from_millis(20));
    assert_eq!(
        s.renderer.control().frame(&color()).unwrap().generation,
        frame.generation
    );
}

#[test]
fn an_edit_restarts_the_image() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let control = s.renderer.control();
    wait_for(&control, samples_at_least(3));

    s.scene
        .stage
        .attribute("/geo.doubleSided")
        .unwrap()
        .set(true)
        .unwrap();
    s.scene.apply();
    s.renderer.update();
    assert_eq!(s.renderer.invalidation(), Invalidation::Restart);

    s.renderer.render(&request(Convergence::Never)).unwrap();
    assert_eq!(s.renderer.invalidation(), Invalidation::None);
    // The render retired the old sampling thread: its last frame is final.
    let before = control.frame(&color()).unwrap();
    assert!(before.samples >= 3);
    let after = next_frame(&s, &color(), before.generation);
    assert!(
        after.samples < before.samples,
        "{} then {}",
        before.samples,
        after.samples
    );
    control.stop();
}

#[test]
fn a_live_setting_keeps_the_samples() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let control = s.renderer.control();
    wait_for(&control, samples_at_least(3));

    s.overrides.set("test:quality", Value::Int(4));
    s.renderer.update();
    assert_eq!(s.renderer.invalidation(), Invalidation::Continue);

    s.renderer.render(&request(Convergence::Never)).unwrap();
    let before = control.frame(&color()).unwrap();
    let after = next_frame(&s, &color(), before.generation);
    assert!(
        after.samples > before.samples,
        "{} then {}",
        before.samples,
        after.samples
    );
    control.stop();
}

#[test]
fn a_live_setting_restarts_without_continue_on_edit() {
    let s = setup(
        SETTINGS_SCENE,
        TestConfig {
            continue_on_edit: false,
            ..slow()
        },
    );
    s.renderer.render(&request(Convergence::Never)).unwrap();
    wait_for(&s.renderer.control(), samples_at_least(1));
    s.overrides.set("test:quality", Value::Int(4));
    s.renderer.update();
    assert_eq!(s.renderer.invalidation(), Invalidation::Restart);
    s.renderer.control().stop();
}

#[test]
fn a_create_only_setting_needs_a_new_renderer() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.overrides
        .set("test:integrator", Value::Token(Token::new("bdpt")));
    s.renderer.update();
    assert_eq!(s.renderer.invalidation(), Invalidation::Recreate);
    assert_eq!(
        s.renderer.render(&RenderRequest::default()).unwrap_err(),
        RenderError::RecreateRequired
    );
}

#[test]
fn a_stop_from_another_thread() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let control = s.renderer.control();
    wait_for(&control, samples_at_least(2));
    let remote = control.clone();
    thread::spawn(move || remote.stop()).join().unwrap();
    let state = control.state();
    assert!(matches!(state, RunState::Stopped { .. }), "{state:?}");
    let frame = control.frame(&color()).unwrap();
    thread::sleep(Duration::from_millis(60));
    assert_eq!(control.frame(&color()).unwrap().generation, frame.generation);
    assert_eq!(control.state(), state);
}

#[test]
fn pause_and_resume() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let control = s.renderer.control();
    wait_for(&control, samples_at_least(2));
    assert!(control.pause());
    // The sample under way may still land.
    thread::sleep(Duration::from_millis(60));
    let RunState::Paused { samples } = control.state() else {
        panic!("not paused: {:?}", control.state());
    };
    thread::sleep(Duration::from_millis(60));
    assert_eq!(control.state(), RunState::Paused { samples });
    assert!(control.resume());
    wait_for(&control, samples_at_least(samples + 1));
    control.stop();
}

#[test]
fn pause_unsupported() {
    let s = setup(
        SETTINGS_SCENE,
        TestConfig {
            pause: false,
            ..slow()
        },
    );
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let control = s.renderer.control();
    wait_for(&control, samples_at_least(1));
    assert!(!control.pause());
    assert!(matches!(control.state(), RunState::Rendering { .. }));
    control.stop();
}

#[test]
fn a_failure_keeps_its_reason_and_frames() {
    let s = setup(
        SETTINGS_SCENE,
        TestConfig {
            fail_at_sample: Some(2),
            ..TestConfig::default()
        },
    );
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let state = converged(&s);
    let RunState::Failed { reason } = state else {
        panic!("not failed: {state:?}");
    };
    assert!(reason.contains("sample 2"), "{reason}");
    assert_eq!(s.renderer.control().frame(&color()).unwrap().samples, 1);
}

#[test]
fn a_crop() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    let crop = PixelRect {
        x: 2,
        y: 1,
        width: 3,
        height: 2,
    };
    s.renderer
        .render(&RenderRequest {
            products: vec![p("/Render/beauty")],
            crop: Some(crop),
            until: Convergence::Samples(1),
            ..RenderRequest::default()
        })
        .unwrap();
    converged(&s);
    let frame = s.renderer.control().frame(&color()).unwrap();
    assert_eq!(frame.data_window, crop);
    assert_eq!(frame.display_window, PixelRect::of_size(8, 4));
    assert_eq!(frame.pixels.len(), 3 * 2 * 12);
}

#[test]
fn a_product_subset() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer
        .render(&RenderRequest {
            products: vec![p("/Render/aovs")],
            until: Convergence::Samples(1),
            ..RenderRequest::default()
        })
        .unwrap();
    converged(&s);
    let output = s.renderer.output();
    assert!(output.prim(&p("/Render/normal")).is_defined());
    assert!(!output.prim(&p("/Render/color")).is_defined());
    assert!(s.renderer.control().frame(&color()).is_none());
}

#[test]
fn unknown_product_and_no_camera() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    let err = s
        .renderer
        .render(&RenderRequest {
            products: vec![p("/Render/nope")],
            ..RenderRequest::default()
        })
        .unwrap_err();
    assert_eq!(err, RenderError::UnknownProduct(p("/Render/nope")));

    let s = setup(CAMERA_SCENE, TestConfig::default());
    assert_eq!(
        s.renderer.render(&RenderRequest::default()).unwrap_err(),
        RenderError::NoCamera
    );
    assert_eq!(s.renderer.control().state(), RunState::Idle);
}

#[test]
fn the_renderer_writes_products() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer
        .render(&RenderRequest {
            until: Convergence::Samples(2),
            writer: ProductWriter::Renderer,
            ..RenderRequest::default()
        })
        .unwrap();
    assert_eq!(converged(&s), RunState::Converged { samples: 2 });
    let beauty = s.scene.dir.path().join("beauty.raw");
    let bytes = std::fs::read(&beauty).unwrap();
    assert!(bytes.starts_with(b"MOHORAW1"));
    assert!(s.scene.dir.path().join("aovs.raw").exists());

    s.renderer.update();
    let data = s.renderer.output().prim(&p("/Render/beauty")).data.unwrap();
    let written = locate(&data, &Locator::parse("renderProductOutput/written"))
        .unwrap()
        .value()
        .unwrap();
    let Value::AssetPath(written) = written else {
        panic!("{written:?}");
    };
    assert_eq!(written.asset_path(), beauty.to_str().unwrap());
}

#[test]
fn the_host_writes_by_default() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer.render(&request(Convergence::Samples(1))).unwrap();
    converged(&s);
    assert!(!s.scene.dir.path().join("beauty.raw").exists());
}

#[test]
fn an_unsupported_writer() {
    let s = setup(
        SETTINGS_SCENE,
        TestConfig {
            writes_products: false,
            ..TestConfig::default()
        },
    );
    let err = s
        .renderer
        .render(&RenderRequest {
            writer: ProductWriter::Renderer,
            ..RenderRequest::default()
        })
        .unwrap_err();
    assert_eq!(err, RenderError::Unsupported("product writing"));
    assert!(err.to_string().contains("product writing"));
}

#[test]
fn commands() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    assert_eq!(s.renderer.invoke(&Token::new("flushTextureCache"), None), Ok(()));
    assert_eq!(
        s.renderer.invoke(&Token::new("explode"), None),
        Err(RenderError::UnknownCommand(Token::new("explode")))
    );
}

#[test]
fn predictable_pixels() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer.render(&request(Convergence::Samples(2))).unwrap();
    converged(&s);
    let first = s.renderer.control().frame(&color()).unwrap();
    s.renderer.render(&request(Convergence::Samples(2))).unwrap();
    converged(&s);
    let second = s.renderer.control().frame(&color()).unwrap();
    assert!(second.generation > first.generation);
    assert_eq!(second.samples, 2);
    assert_eq!(first.pixels, second.pixels);
}

#[test]
fn pixels_follow_the_scene() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer.render(&request(Convergence::Samples(2))).unwrap();
    converged(&s);
    let before = s.renderer.control().frame(&color()).unwrap();
    s.scene
        .stage
        .attribute("/geo.doubleSided")
        .unwrap()
        .set(true)
        .unwrap();
    s.scene.apply();
    s.renderer.update();
    s.renderer.render(&request(Convergence::Samples(2))).unwrap();
    converged(&s);
    let after = s.renderer.control().frame(&color()).unwrap();
    assert_ne!(before.pixels, after.pixels);
}

#[test]
fn the_output_follows_the_render() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    s.renderer.render(&request(Convergence::Samples(3))).unwrap();
    converged(&s);
    let output = s.renderer.output();
    let read = |path: &str, locator: &str| {
        locate(&output.prim(&p(path)).data.unwrap(), &Locator::parse(locator)).and_then(|d| d.value())
    };
    // Nothing reported before the owning thread polls.
    assert_eq!(
        read("/", "renderer/state"),
        Some(Value::Token(Token::new("idle")))
    );
    s.renderer.poll();
    assert_eq!(
        read("/", "renderer/state"),
        Some(Value::Token(Token::new("converged")))
    );
    assert_eq!(read("/", "renderer/samples"), Some(Value::Int(3)));
    assert_eq!(
        read("/Render/color", "renderBuffer/converged"),
        Some(Value::Bool(true))
    );
    let generation = s.renderer.control().frame(&color()).unwrap().generation as i64;
    assert_eq!(
        read("/Render/color", "renderBuffer/generation"),
        Some(Value::Int64(generation))
    );
}

/// A frame shows the view its render started with, not a later edit.
#[test]
fn a_camera_edited_after_the_render_started() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Never)).unwrap();
    let control = s.renderer.control();
    wait_for(&control, samples_at_least(1));
    let before = control.frame(&color()).unwrap();
    let view = before.view.clone().unwrap();
    assert_eq!(view.camera, p("/cam"));
    // Unauthored: the schema's fallback, 50 tenths of a unit, as the index gives it.
    assert_eq!(view.focal_length, Some(5.0));

    s.scene
        .stage
        .prim("/cam")
        .unwrap()
        .create_attribute("focalLength", "float")
        .unwrap()
        .set(35.0f32)
        .unwrap();
    s.scene.apply();
    s.renderer.update();
    let later = next_frame(&s, &color(), before.generation);
    assert_eq!(
        later.view.as_deref(),
        Some(&*view),
        "the render's view, not the edit"
    );

    s.renderer.render(&request(Convergence::Never)).unwrap();
    let restarted = wait_for(&control, samples_at_least(1));
    assert!(restarted.is_active());
    let mut frame = control.frame(&color()).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while frame.render == before.render {
        assert!(Instant::now() < deadline, "a frame of the new render");
        thread::sleep(Duration::from_millis(1));
        frame = control.frame(&color()).unwrap();
    }
    let focal = frame.view.as_ref().and_then(|v| v.focal_length).unwrap();
    assert!(
        (focal - 3.5).abs() < 1e-6,
        "the next render reads the edit: {focal}"
    );
    control.stop();
}

/// Each frame gives the index's time and the settings' colour space.
#[test]
fn frames_give_time_and_colour_space() {
    let text = SETTINGS_SCENE.replace(
        "int test:quality = 64",
        "int test:quality = 64\n        uniform token renderingColorSpace = \"acescg\"",
    );
    let s = setup(&text, TestConfig::default());
    s.scene.globals.set_current_frame(12.0);
    s.scene.apply();
    s.renderer.update();
    s.renderer.render(&request(Convergence::Samples(2))).unwrap();
    converged(&s);
    let frame = s.renderer.control().frame(&color()).unwrap();
    let view = frame.view.as_ref().unwrap();
    assert_eq!(view.time, Some(12.0));
    assert_eq!(
        view.rendering_color_space.as_ref().map(|t| t.as_str()),
        Some("acescg")
    );
}

/// A stage with one mesh and a product of `color` and `primId` vars.
const IDS_SCENE: &str = r#"#usda 1.0
(
    renderSettingsPrimPath = "/Render/settings"
)
def Camera "cam"
{
}
def Xform "World"
{
    def Mesh "A"
    {
    }
}
def Scope "Render"
{
    def RenderSettings "settings"
    {
        rel camera = </cam>
        int2 resolution = (8, 4)
        rel products = [</Render/beauty>]
    }
    def RenderProduct "beauty"
    {
        rel orderedVars = [</Render/color>, </Render/ids>]
    }
    def RenderVar "color"
    {
        uniform string sourceName = "color"
    }
    def RenderVar "ids"
    {
        uniform token dataType = "int"
        uniform string sourceName = "primId"
    }
}
"#;

fn id_at(frame: &moho_render::Frame, x: u32, y: u32) -> i32 {
    let at = ((y * frame.data_window.width + x) * 4) as usize;
    i32::from_le_bytes(frame.pixels[at..at + 4].try_into().unwrap())
}

/// A host picks from the identifier frame alone: the centre pixel names the
/// mesh, a background pixel nothing.
#[test]
fn a_pick_from_a_frame() {
    let s = setup(IDS_SCENE, TestConfig::default());
    assert!(
        s.renderer
            .capabilities()
            .aovs
            .iter()
            .any(|a| a.name.as_str() == "primId" && a.format == moho_render::PixelFormat::I32)
    );
    s.renderer.render(&request(Convergence::Samples(2))).unwrap();
    converged(&s);
    let frame = s.renderer.control().frame(&p("/Render/ids")).unwrap();
    assert_eq!(frame.format, moho_render::PixelFormat::I32);
    let table = frame.ids.clone().unwrap();
    assert_eq!(table.path(id_at(&frame, 4, 2)), Some(&p("/World/A")));
    // The background.
    let background = id_at(&frame, 0, 0);
    assert_eq!(table.path(background), None);
}

/// Every identifier a frame holds names a prim the terminal index holds.
#[test]
fn identifiers_name_scene_prims() {
    let text = IDS_SCENE.replace(
        "def Mesh \"A\"\n    {\n    }",
        "def Mesh \"A\"\n    {\n    }\n    def Mesh \"B\"\n    {\n    }\n    def Mesh \"C\"\n    {\n    }",
    );
    let s = setup(&text, TestConfig::default());
    s.renderer.render(&request(Convergence::Samples(1))).unwrap();
    converged(&s);
    let frame = s.renderer.control().frame(&p("/Render/ids")).unwrap();
    let table = frame.ids.clone().unwrap();
    assert_eq!(table.len(), 3);
    let mut seen = std::collections::BTreeSet::new();
    for y in 0..frame.data_window.height {
        for x in 0..frame.data_window.width {
            let id = id_at(&frame, x, y);
            if let Some(path) = table.path(id) {
                assert!(
                    s.scene.terminal.prim(path).data.is_some(),
                    "{path} is in the index"
                );
                seen.insert(path.clone());
            } else {
                assert_eq!(id, moho_render::testing::BACKGROUND);
            }
        }
    }
    assert_eq!(seen.len(), 3, "every mesh shows");
}

/// The memory the renderer holds, read from another thread and from the
/// output index after a poll.
#[test]
fn memory_read_from_another_thread() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    let control = s.renderer.control();
    let stats = thread::spawn(move || control.stats()).join().unwrap();
    let host = stats.get(&Token::new("hostBytes")).cloned().unwrap();
    assert!(matches!(host, Value::Int64(bytes) if bytes > 0), "{host:?}");
    assert_eq!(stats.get(&Token::new("deviceBytes")), Some(&Value::Int64(0)));

    s.renderer.poll();
    let output = s.renderer.output();
    let root = output.prim(&sdf::Path::abs_root()).data.unwrap();
    let read = |key: &str| locate(&root, &Locator::new(["renderer", "stats", key])).and_then(|d| d.value());
    assert_eq!(read("hostBytes"), Some(host));
    assert_eq!(read("deviceBytes"), Some(Value::Int64(0)));
}

/// Every var refines while the render runs, not only at convergence, and a
/// pass's frames share its sample count.
#[test]
fn every_var_refines() {
    let s = setup(SETTINGS_SCENE, slow());
    s.renderer.render(&request(Convergence::Samples(8))).unwrap();
    let control = s.renderer.control();
    let (mut color_samples, mut depth_samples) = (Vec::new(), Vec::new());
    let deadline = Instant::now() + TIMEOUT;
    while depth_samples.iter().all(|&n| n < 2) {
        assert!(Instant::now() < deadline, "depth refines");
        if let Some(frame) = control.frame(&color()) {
            color_samples.push(frame.samples);
        }
        if let Some(frame) = control.frame(&p("/Render/depth")) {
            depth_samples.push(frame.samples);
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        matches!(control.state(), RunState::Rendering { .. }),
        "depth had frames before convergence"
    );
    control.stop();
    let color_now = control.frame(&color()).unwrap().samples;
    for n in &depth_samples {
        assert!(*n <= color_now, "depth at {n} samples, color at {color_now}");
    }
    let last_depth = control.frame(&p("/Render/depth")).unwrap().samples;
    assert!(
        (color_now as i64 - last_depth as i64).abs() <= 1,
        "one pass apart at most once stopped: color {color_now}, depth {last_depth}"
    );
}

/// An update names its class: a host reads `unchanged` and `full` alike from
/// every renderer, and any other token as the renderer's own.
#[test]
fn an_update_names_its_class() {
    let s = setup(SETTINGS_SCENE, TestConfig::default());
    let class = || {
        s.renderer
            .control()
            .stats()
            .get(&Token::new(moho_render::tokens::stats::UPDATE_CLASS))
            .cloned()
    };
    s.renderer.update();
    assert_eq!(class(), Some(Value::Token(Token::new("unchanged"))));
    s.scene
        .stage
        .attribute("/geo.doubleSided")
        .unwrap()
        .set(true)
        .unwrap();
    s.scene.apply();
    s.renderer.update();
    assert_eq!(class(), Some(Value::Token(Token::new("restart"))));
}
