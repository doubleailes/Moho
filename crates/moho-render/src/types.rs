//! The values render control passes around: invalidation, run states,
//! frames, requests and errors.

use std::fmt;
use std::sync::{Arc, OnceLock};

use moho::openusd::sdf;
use moho::openusd::tf::Token;

use crate::device::{DeviceImageHandle, ReadbackError};
use crate::targets::RenderView;

/// What the updates since the last render cost the image in flight.
///
/// Ordered from cheapest to costliest, so the cost of several updates is
/// their [`max`](Ord::max).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Invalidation {
    /// Nothing the image depends on changed: convergence continues.
    #[default]
    None,
    /// The samples so far stay valid (a renderer that can blend or reweight
    /// them, after a light's intensity changed, say).
    Continue,
    /// The image restarts from zero samples.
    Restart,
    /// Something the renderer reads only when it is created changed (the
    /// integrator kind, say): the host recreates the renderer.
    Recreate,
}

/// Where a renderer is in its work. Readable from any thread through the
/// [`ControlHandle`](crate::ControlHandle).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RunState {
    /// No render started yet.
    #[default]
    Idle,
    /// A render started and its first sample is not done.
    Loading,
    /// Sampling, with the samples done and, when the renderer knows it, the
    /// fraction of the stop condition reached.
    Rendering { samples: u32, progress: Option<f32> },
    /// Paused by the host, keeping its samples.
    Paused { samples: u32 },
    /// The stop condition was met.
    Converged { samples: u32 },
    /// Stopped by the host before its stop condition.
    Stopped { samples: u32 },
    /// The render failed; frames already made stay readable.
    Failed { reason: Arc<str> },
}

impl RunState {
    /// The state's kind, as `renderer/state` names it: `idle`, `loading`,
    /// `rendering`, `paused`, `converged`, `stopped` or `failed`.
    pub fn token(&self) -> Token {
        Token::new(match self {
            RunState::Idle => "idle",
            RunState::Loading => "loading",
            RunState::Rendering { .. } => "rendering",
            RunState::Paused { .. } => "paused",
            RunState::Converged { .. } => "converged",
            RunState::Stopped { .. } => "stopped",
            RunState::Failed { .. } => "failed",
        })
    }

    /// Whether `other` is the same kind of state, whatever its samples or
    /// progress.
    pub fn same_kind(&self, other: &RunState) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    /// The samples done, where the state has them.
    pub fn samples(&self) -> Option<u32> {
        match self {
            RunState::Rendering { samples, .. }
            | RunState::Paused { samples }
            | RunState::Converged { samples }
            | RunState::Stopped { samples } => Some(*samples),
            RunState::Idle | RunState::Loading | RunState::Failed { .. } => None,
        }
    }

    /// The fraction of the stop condition reached, when known.
    pub fn progress(&self) -> Option<f32> {
        match self {
            RunState::Rendering { progress, .. } => *progress,
            RunState::Converged { .. } => Some(1.0),
            _ => None,
        }
    }

    /// Whether a render is under way: loading, rendering or paused.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            RunState::Loading | RunState::Rendering { .. } | RunState::Paused { .. }
        )
    }
}

/// A pixel format, after `HdFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Four half floats (`HdFormatFloat16Vec4`).
    F16x4,
    /// Four floats (`HdFormatFloat32Vec4`).
    F32x4,
    /// Three floats (`HdFormatFloat32Vec3`).
    F32x3,
    /// One float (`HdFormatFloat32`).
    F32,
    /// One 32-bit integer, for ids (`HdFormatInt32`).
    I32,
    /// Four normalized bytes (`HdFormatUNorm8Vec4`).
    U8x4,
}

impl PixelFormat {
    /// The bytes one pixel takes.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::F16x4 => 8,
            PixelFormat::F32x4 => 16,
            PixelFormat::F32x3 => 12,
            PixelFormat::F32 | PixelFormat::I32 | PixelFormat::U8x4 => 4,
        }
    }

    /// The format's name, as `renderBuffer/format` gives it.
    pub fn token(self) -> Token {
        Token::new(match self {
            PixelFormat::F16x4 => "float16Vec4",
            PixelFormat::F32x4 => "float32Vec4",
            PixelFormat::F32x3 => "float32Vec3",
            PixelFormat::F32 => "float32",
            PixelFormat::I32 => "int32",
            PixelFormat::U8x4 => "unorm8Vec4",
        })
    }

    /// The format for a render var's `dataType` (`color3f`, `float`, ...),
    /// four floats for a type it does not know.
    pub fn for_data_type(data_type: &str) -> PixelFormat {
        match data_type {
            "float" | "double" => PixelFormat::F32,
            "int" | "uint" | "int64" => PixelFormat::I32,
            "color3f" | "normal3f" | "vector3f" | "point3f" | "float3" => PixelFormat::F32x3,
            "half4" | "color4h" => PixelFormat::F16x4,
            "uchar4" => PixelFormat::U8x4,
            _ => PixelFormat::F32x4,
        }
    }
}

/// A rectangle of pixels, the top-left pixel at `(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    /// The rectangle of an image of `width` by `height` pixels.
    pub fn of_size(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// The pixels it covers.
    pub fn area(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Whether `other` lies inside this rectangle.
    pub fn contains(&self, other: &PixelRect) -> bool {
        let right = i64::from(self.x) + i64::from(self.width);
        let bottom = i64::from(self.y) + i64::from(self.height);
        other.x >= self.x
            && other.y >= self.y
            && i64::from(other.x) + i64::from(other.width) <= right
            && i64::from(other.y) + i64::from(other.height) <= bottom
    }
}

/// Where a frame's pixels are.
#[derive(Clone)]
pub enum FramePixels {
    /// In host memory: the data window's pixels, rows top to bottom,
    /// tightly packed, multisampling already resolved.
    Host(Arc<[u8]>),
    /// On a graphics device, read back on demand.
    Device(DeviceImageHandle),
}

impl fmt::Debug for FramePixels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FramePixels::Host(bytes) => write!(f, "Host({} bytes)", bytes.len()),
            FramePixels::Device(image) => write!(f, "Device({})", image.backend().as_str()),
        }
    }
}

impl PartialEq for FramePixels {
    /// Host pixels by value, device images by identity.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FramePixels::Host(a), FramePixels::Host(b)) => a == b,
            (FramePixels::Device(a), FramePixels::Device(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// One render var's pixels at one point in time: an immutable snapshot, so
/// a reader never locks the renderer and a later snapshot never alters it.
/// Made with [`Frame::new`] or [`Frame::on_device`], so a field added later
/// breaks no renderer.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Frame {
    /// Grows with each snapshot of the var. Set when the frame is published.
    pub generation: u64,
    /// The samples it holds.
    pub samples: u32,
    pub format: PixelFormat,
    /// The whole image.
    pub display_window: PixelRect,
    /// Where these pixels are in the display window.
    pub data_window: PixelRect,
    /// The data window's pixels, in host memory or on a device: read them
    /// with [`read_pixels`](Self::read_pixels) or
    /// [`read_rect`](Self::read_rect) wherever they are.
    pub pixels: FramePixels,
    /// The render that made it: grows across the renderer's renders. Set
    /// when the frame is published.
    pub render: u64,
    /// The view its render read when it started, shared by the render's
    /// frames of one product.
    pub view: Option<Arc<RenderView>>,
    /// For an identifier var (`primId`, `instanceId`): the prim each
    /// identifier its pixels hold names, shared by the render's frames of
    /// the var.
    pub ids: Option<Arc<IdTable>>,
    /// A device frame's pixels once read back whole, so a second read
    /// transfers nothing.
    read_back: OnceLock<Arc<[u8]>>,
}

impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.samples == other.samples
            && self.format == other.format
            && self.display_window == other.display_window
            && self.data_window == other.data_window
            && self.pixels == other.pixels
            && self.render == other.render
            && self.view == other.view
            && self.ids == other.ids
    }
}

impl Frame {
    /// A frame of `samples` samples, holding `pixels` of `format` for
    /// `data_window` in `display_window`. Its generation is set when it is
    /// published.
    pub fn new(
        format: PixelFormat,
        display_window: PixelRect,
        data_window: PixelRect,
        samples: u32,
        pixels: Arc<[u8]>,
    ) -> Self {
        Self::with_pixels(
            format,
            display_window,
            data_window,
            samples,
            FramePixels::Host(pixels),
        )
    }

    /// A frame of `samples` samples whose pixels are `image`, on a device,
    /// for `data_window` in `display_window`. The image holds `format` and
    /// the data window's size.
    pub fn on_device(
        format: PixelFormat,
        display_window: PixelRect,
        data_window: PixelRect,
        samples: u32,
        image: DeviceImageHandle,
    ) -> Self {
        Self::with_pixels(
            format,
            display_window,
            data_window,
            samples,
            FramePixels::Device(image),
        )
    }

    fn with_pixels(
        format: PixelFormat,
        display_window: PixelRect,
        data_window: PixelRect,
        samples: u32,
        pixels: FramePixels,
    ) -> Self {
        Self {
            generation: 0,
            samples,
            format,
            display_window,
            data_window,
            pixels,
            render: 0,
            view: None,
            ids: None,
            read_back: OnceLock::new(),
        }
    }

    /// This frame, its identifiers naming the prims of `ids`.
    pub fn with_ids(mut self, ids: Arc<IdTable>) -> Self {
        self.ids = Some(ids);
        self
    }

    /// This frame, showing `view`.
    pub fn with_view(mut self, view: Arc<RenderView>) -> Self {
        self.view = Some(view);
        self
    }

    /// The device image holding its pixels, `None` for a host frame.
    pub fn device_image(&self) -> Option<&DeviceImageHandle> {
        match &self.pixels {
            FramePixels::Device(image) => Some(image),
            FramePixels::Host(_) => None,
        }
    }

    /// The data window's pixels in host memory, rows top to bottom, tightly
    /// packed in [`format`](Self::format). A host frame gives its own bytes;
    /// a device frame reads them back the first time and keeps them.
    pub fn read_pixels(&self) -> Result<Arc<[u8]>, ReadbackError> {
        match &self.pixels {
            FramePixels::Host(bytes) => Ok(bytes.clone()),
            FramePixels::Device(image) => {
                if let Some(bytes) = self.read_back.get() {
                    return Ok(bytes.clone());
                }
                let window = PixelRect::of_size(self.data_window.width, self.data_window.height);
                let bytes: Arc<[u8]> = image.read(window)?.into();
                // Another thread may have read it meanwhile: keep the first.
                Ok(self.read_back.get_or_init(|| bytes).clone())
            }
        }
    }

    /// The pixels of `rect`, given in the display window's coordinates as
    /// the data window is, rows top to bottom, tightly packed. A device
    /// frame not yet read back whole reads `rect` alone (one pixel under a
    /// pick, say). A rectangle reaching past the data window is an error.
    pub fn read_rect(&self, rect: PixelRect) -> Result<Vec<u8>, ReadbackError> {
        if !self.data_window.contains(&rect) {
            return Err(ReadbackError::OutsideWindow {
                rect,
                window: self.data_window,
            });
        }
        let local = PixelRect {
            x: rect.x - self.data_window.x,
            y: rect.y - self.data_window.y,
            width: rect.width,
            height: rect.height,
        };
        let whole = match &self.pixels {
            FramePixels::Host(bytes) => bytes.clone(),
            FramePixels::Device(image) => match self.read_back.get() {
                Some(bytes) => bytes.clone(),
                None => return image.read(local),
            },
        };
        Ok(crop(
            &whole,
            self.data_window.width,
            self.format.bytes_per_pixel(),
            local,
        ))
    }
}

/// The rows of `rect` out of an image `width` pixels wide.
fn crop(bytes: &[u8], width: u32, pixel: usize, rect: PixelRect) -> Vec<u8> {
    let row = width as usize * pixel;
    let (x, y) = (rect.x.max(0) as usize, rect.y.max(0) as usize);
    let span = rect.width as usize * pixel;
    let mut out = Vec::with_capacity(span * rect.height as usize);
    for r in y..y + rect.height as usize {
        let start = r * row + x * pixel;
        if let Some(line) = bytes.get(start..start + span) {
            out.extend_from_slice(line);
        }
    }
    out
}

/// The prims an identifier var's pixels name: each identifier to the
/// absolute path of the prim the renderer attributes the hit to (for an
/// instance, the instance or instancer prim, as the renderer names it). The
/// value that marks no hit is not in the table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IdTable(Vec<(i32, sdf::Path)>);

impl IdTable {
    /// The table of `entries`; an identifier given twice keeps its first
    /// path.
    pub fn new(entries: impl IntoIterator<Item = (i32, sdf::Path)>) -> Self {
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_by_key(|(id, _)| *id);
        entries.dedup_by_key(|(id, _)| *id);
        Self(entries)
    }

    /// The prim `id` names, `None` for an identifier the table lacks (the
    /// background among them).
    pub fn path(&self, id: i32) -> Option<&sdf::Path> {
        let at = self.0.binary_search_by_key(&id, |(i, _)| *i).ok()?;
        self.0.get(at).map(|(_, path)| path)
    }

    /// The identifiers and their prims, by identifier.
    pub fn iter(&self) -> impl Iterator<Item = (i32, &sdf::Path)> {
        self.0.iter().map(|(id, path)| (*id, path))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// When a render stops sampling.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Convergence {
    /// When the renderer judges the image converged.
    #[default]
    Converged,
    /// After this many samples.
    Samples(u32),
    /// After this many seconds.
    Seconds(f32),
    /// Never: until the host stops it.
    Never,
}

/// Who writes the products when the render converges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProductWriter {
    /// The host, from frames. The renderer writes nothing.
    #[default]
    Host,
    /// The renderer, through its own drivers, to each product's name.
    Renderer,
}

/// A render's per-call overrides: what the host asks now. What to render is
/// read from the scene ([`targets`](crate::targets)).
#[derive(Debug, Clone, Default)]
pub struct RenderRequest {
    /// Render only these products (every product when empty).
    pub products: Vec<sdf::Path>,
    /// A window in the display window to render, in pixels.
    pub crop: Option<PixelRect>,
    /// When to stop sampling.
    pub until: Convergence,
    /// Who writes the products.
    pub writer: ProductWriter,
}

/// Why render control failed.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    /// No camera to render through: no settings camera, no product camera
    /// and no primary camera.
    NoCamera,
    /// A requested product is not among those the scene offers.
    UnknownProduct(sdf::Path),
    /// The renderer lacks a capability the request needs.
    Unsupported(&'static str),
    /// A change since the last render needs the renderer recreated.
    RecreateRequired,
    /// The renderer declares no such command.
    UnknownCommand(Token),
    /// The request is malformed.
    InvalidRequest(String),
    /// The renderer failed.
    Failed(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::NoCamera => f.write_str(
                "no camera to render through: no product or settings camera, and no primary camera",
            ),
            RenderError::UnknownProduct(path) => write!(f, "unknown render product {path}"),
            RenderError::Unsupported(capability) => {
                write!(f, "the renderer does not support {capability}")
            }
            RenderError::RecreateRequired => {
                f.write_str("a change needs the renderer recreated before it renders again")
            }
            RenderError::UnknownCommand(name) => write!(f, "unknown renderer command {}", name.as_str()),
            RenderError::InvalidRequest(reason) => write!(f, "invalid render request: {reason}"),
            RenderError::Failed(reason) => write!(f, "render failed: {reason}"),
        }
    }
}

impl std::error::Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> sdf::Path {
        sdf::path(s).unwrap()
    }

    #[test]
    fn an_id_table_looks_up_by_identifier() {
        let table = IdTable::new([(7, p("/b")), (3, p("/a")), (7, p("/c"))]);
        assert_eq!(table.len(), 2);
        assert_eq!(table.path(3), Some(&p("/a")));
        assert_eq!(table.path(7), Some(&p("/b")), "the first path given wins");
        assert_eq!(table.path(0), None, "the background is no entry");
        assert_eq!(table.iter().map(|(id, _)| id).collect::<Vec<_>>(), [3, 7]);
    }

    #[test]
    fn invalidation_order() {
        assert!(Invalidation::None < Invalidation::Continue);
        assert!(Invalidation::Continue < Invalidation::Restart);
        assert!(Invalidation::Restart < Invalidation::Recreate);
        assert_eq!(Invalidation::default(), Invalidation::None);
    }

    #[test]
    fn the_strongest_invalidation_wins() {
        let folded = [Invalidation::Continue, Invalidation::Restart, Invalidation::None]
            .into_iter()
            .fold(Invalidation::None, Ord::max);
        assert_eq!(folded, Invalidation::Restart);
    }

    #[test]
    fn state_tokens() {
        let states = [
            (RunState::Idle, "idle"),
            (RunState::Loading, "loading"),
            (
                RunState::Rendering {
                    samples: 1,
                    progress: None,
                },
                "rendering",
            ),
            (RunState::Paused { samples: 1 }, "paused"),
            (RunState::Converged { samples: 1 }, "converged"),
            (RunState::Stopped { samples: 1 }, "stopped"),
            (RunState::Failed { reason: "x".into() }, "failed"),
        ];
        for (state, token) in states {
            assert_eq!(state.token().as_str(), token);
        }
    }

    #[test]
    fn states_of_one_kind() {
        let a = RunState::Rendering {
            samples: 1,
            progress: Some(0.1),
        };
        let b = RunState::Rendering {
            samples: 7,
            progress: None,
        };
        assert!(a.same_kind(&b));
        assert!(!a.same_kind(&RunState::Converged { samples: 1 }));
    }

    #[test]
    fn errors_name_their_subject() {
        assert!(RenderError::NoCamera.to_string().contains("camera"));
        let product = RenderError::UnknownProduct(p("/Render/nope")).to_string();
        assert!(product.contains("/Render/nope"), "{product}");
        let command = RenderError::UnknownCommand(Token::new("explode")).to_string();
        assert!(command.contains("explode"), "{command}");
        let unsupported = RenderError::Unsupported("product writing").to_string();
        assert!(unsupported.contains("product writing"), "{unsupported}");
    }

    #[test]
    fn rect_containment() {
        let display = PixelRect::of_size(8, 4);
        assert!(display.contains(&PixelRect {
            x: 2,
            y: 1,
            width: 4,
            height: 3
        }));
        assert!(!display.contains(&PixelRect {
            x: 6,
            y: 0,
            width: 4,
            height: 1
        }));
        assert!(!display.contains(&PixelRect {
            x: -1,
            y: 0,
            width: 1,
            height: 1
        }));
    }
}
