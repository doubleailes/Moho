//! Device frames and drivers, over the test renderer's test backend: one
//! test per scenario of the `render-control` spec's device requirements.

#![allow(clippy::unwrap_used)]

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use common::{Scene, p};
use moho::chain::{InsertionOrder, RendererFilter, create_renderer};
use moho::openusd::tf::Token;
use moho::renderer::{CreateRendererError, Renderer, RendererPlugin, Unsupported};
use moho::scene_index::SceneIndexHandle;
use moho::schema::RendererCreateArgsSchema;
use moho_render::testing::{HostImage, TestConfig, TestPlugin, TestRenderer};
use moho_render::tokens::driver::RENDER_DRIVER;
use moho_render::{
    ControlledRenderer, Convergence, Drivers, Frame, PixelFormat, PixelRect, ReadbackError, RenderRequest,
    RunState,
};

/// One mesh (the right half of the identifier frame), and a product of `color` and
/// `primId` vars.
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
        int2 resolution = (8, 6)
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

fn render(device_frames: bool) -> (Scene, TestRenderer) {
    let scene = Scene::new(IDS_SCENE);
    let config = TestConfig {
        device_frames,
        ..TestConfig::default()
    };
    let created = create_renderer(
        scene.terminal.clone(),
        &[],
        &TestPlugin::new(config),
        &RendererCreateArgsSchema::empty(),
    )
    .unwrap();
    let renderer = created.renderer;
    renderer.update();
    renderer
        .render(&RenderRequest {
            until: Convergence::Samples(2),
            ..RenderRequest::default()
        })
        .unwrap();
    let control = renderer.control();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut state = control.state();
    while state.is_active() || state == RunState::Idle {
        assert!(Instant::now() < deadline, "the render did not finish: {state:?}");
        state = control.wait(&state, Duration::from_millis(50));
    }
    (scene, renderer)
}

fn test_image(frame: &Frame) -> &HostImage {
    frame
        .device_image()
        .unwrap()
        .as_any()
        .downcast_ref::<HostImage>()
        .unwrap()
}

#[test]
fn a_device_frame_names_its_backend() {
    let (_scene, renderer) = render(true);
    let frame = renderer.control().frame(&p("/Render/color")).unwrap();
    let image = frame.device_image().unwrap();
    assert_eq!(image.backend().as_str(), HostImage::BACKEND);
    assert_eq!(image.format(), frame.format);
    assert_eq!(image.size(), (8, 6));
}

#[test]
fn a_held_device_frame_stays_valid() {
    let (scene, renderer) = render(true);
    let held = renderer.control().frame(&p("/Render/color")).unwrap();
    let before = held.read_pixels().unwrap();
    scene
        .stage
        .attribute("/World/A.doubleSided")
        .unwrap()
        .set(true)
        .unwrap();
    scene.apply();
    renderer.update();
    renderer
        .render(&RenderRequest {
            until: Convergence::Samples(2),
            ..RenderRequest::default()
        })
        .unwrap();
    let control = renderer.control();
    let mut state = control.state();
    let deadline = Instant::now() + Duration::from_secs(20);
    while state.is_active() {
        assert!(Instant::now() < deadline);
        state = control.wait(&state, Duration::from_millis(50));
    }
    let newer = control.frame(&p("/Render/color")).unwrap();
    assert!(newer.generation > held.generation);
    assert_ne!(newer.read_pixels().unwrap(), before, "the scene changed");
    assert_eq!(held.read_pixels().unwrap(), before);
}

#[test]
fn the_whole_frame_reads_back_once() {
    let (_scene, renderer) = render(true);
    let frame = renderer.control().frame(&p("/Render/color")).unwrap();
    let first = frame.read_pixels().unwrap();
    let second = frame.read_pixels().unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 8 * 6 * frame.format.bytes_per_pixel());
    assert_eq!(test_image(&frame).reads(), 1, "the second read transfers nothing");
}

#[test]
fn same_pixels_on_either_side() {
    let (_host_scene, host) = render(false);
    let (_device_scene, device) = render(true);
    for var in ["/Render/color", "/Render/ids"] {
        let host_frame = host.control().frame(&p(var)).unwrap();
        let device_frame = device.control().frame(&p(var)).unwrap();
        assert!(host_frame.device_image().is_none());
        assert_eq!(
            device_frame.device_image().unwrap().backend().as_str(),
            HostImage::BACKEND
        );
        assert_eq!(
            host_frame.read_pixels().unwrap(),
            device_frame.read_pixels().unwrap(),
            "{var}"
        );
    }
}

#[test]
fn one_pixel_under_a_pick() {
    let (_scene, renderer) = render(true);
    let frame = renderer.control().frame(&p("/Render/ids")).unwrap();
    let pixel = frame
        .read_rect(PixelRect {
            x: 5,
            y: 3,
            width: 1,
            height: 1,
        })
        .unwrap();
    assert_eq!(pixel.len(), 4);
    let id = i32::from_le_bytes(pixel.try_into().unwrap());
    assert_eq!(frame.ids.as_ref().unwrap().path(id), Some(&p("/World/A")));
    assert_eq!(test_image(&frame).reads(), 1, "one pixel, not the whole frame");
    let whole = frame.read_pixels().unwrap();
    let at = (3 * 8 + 5) * 4;
    assert_eq!(
        whole[at..at + 4],
        id.to_le_bytes(),
        "the rectangle is the whole frame's pixel"
    );
}

#[test]
fn a_rectangle_outside_the_window_is_an_error() {
    let (_scene, renderer) = render(true);
    let frame = renderer.control().frame(&p("/Render/ids")).unwrap();
    let rect = PixelRect {
        x: 7,
        y: 0,
        width: 2,
        height: 1,
    };
    assert_eq!(
        frame.read_rect(rect),
        Err(ReadbackError::OutsideWindow {
            rect,
            window: frame.data_window
        })
    );
    assert_eq!(test_image(&frame).reads(), 0);
}

#[test]
fn a_host_frame_reads_through_the_same_accessor() {
    let pixels: std::sync::Arc<[u8]> = (0..16).collect::<Vec<u8>>().into();
    let window = PixelRect::of_size(2, 2);
    let frame = Frame::new(PixelFormat::I32, window, window, 1, pixels.clone());
    assert!(
        std::sync::Arc::ptr_eq(&frame.read_pixels().unwrap(), &pixels),
        "no copy"
    );
    let rect = PixelRect {
        x: 1,
        y: 1,
        width: 1,
        height: 1,
    };
    assert_eq!(frame.read_rect(rect).unwrap(), [12, 13, 14, 15]);
}

#[test]
fn a_device_frame_in_a_display_window() {
    let display = PixelRect::of_size(8, 6);
    let data = PixelRect {
        x: 2,
        y: 1,
        width: 2,
        height: 2,
    };
    let bytes: std::sync::Arc<[u8]> = (0..16).collect::<Vec<u8>>().into();
    let image = std::sync::Arc::new(HostImage::new(PixelFormat::I32, (2, 2), bytes));
    let frame = Frame::on_device(PixelFormat::I32, display, data, 1, image);
    let pixel = frame
        .read_rect(PixelRect {
            x: 3,
            y: 2,
            width: 1,
            height: 1,
        })
        .unwrap();
    assert_eq!(
        pixel,
        [12, 13, 14, 15],
        "display coordinates, the data window's origin removed"
    );
}

#[test]
fn another_backend_gives_nothing() {
    struct OtherImage;
    let (_scene, renderer) = render(true);
    let frame = renderer.control().frame(&p("/Render/color")).unwrap();
    assert!(
        frame
            .device_image()
            .unwrap()
            .as_any()
            .downcast_ref::<OtherImage>()
            .is_none()
    );
    assert!(frame.read_pixels().is_ok(), "still read back");
}

/// A plugin over the test renderer that needs a `renderDriver` of type
/// `Device`, and records whether its filters were asked for.
struct NeedsDevice {
    drivers: Drivers,
    filters_asked: Rc<Cell<bool>>,
}

struct Device;

impl RendererPlugin for NeedsDevice {
    type Renderer = TestRenderer;

    fn is_supported(&self, _: &RendererCreateArgsSchema) -> Result<(), Unsupported> {
        match self.drivers.get::<Device>(&Token::new(RENDER_DRIVER)) {
            Some(_) => Ok(()),
            None => Err(Unsupported::new(format!("no {RENDER_DRIVER} given"))),
        }
    }

    fn renderer_filters(&self) -> Vec<RendererFilter> {
        self.filters_asked.set(true);
        Vec::new()
    }

    fn create_renderer(
        &self,
        terminal: SceneIndexHandle,
        _: &RendererCreateArgsSchema,
    ) -> Option<TestRenderer> {
        Some(TestRenderer::new(terminal, TestConfig::default()))
    }
}

#[test]
fn a_gpu_renderer_without_a_device() {
    let scene = Scene::new(IDS_SCENE);
    let built = Rc::new(Cell::new(false));
    let host = {
        let built = built.clone();
        vec![RendererFilter::new(0, InsertionOrder::AtEnd, move |input| {
            built.set(true);
            input
        })]
    };
    let plugin = NeedsDevice {
        drivers: Drivers::new(),
        filters_asked: Rc::default(),
    };
    let created = create_renderer(
        scene.terminal.clone(),
        &host,
        &plugin,
        &RendererCreateArgsSchema::empty(),
    );
    match created {
        Err(CreateRendererError::Unsupported(unsupported)) => {
            assert!(unsupported.to_string().contains(RENDER_DRIVER), "{unsupported}");
        }
        Err(other) => panic!("expected unsupported, got {other}"),
        Ok(_) => panic!("expected unsupported, got a renderer"),
    }
    assert!(!built.get() && !plugin.filters_asked.get(), "no filter was built");

    let plugin = NeedsDevice {
        drivers: Drivers::new().with(RENDER_DRIVER, Device),
        filters_asked: Rc::default(),
    };
    let created = create_renderer(
        scene.terminal.clone(),
        &host,
        &plugin,
        &RendererCreateArgsSchema::empty(),
    );
    assert!(created.is_ok());
    assert!(built.get());
}
