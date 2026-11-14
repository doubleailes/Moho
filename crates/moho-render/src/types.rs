//! The values render control passes around: invalidation, run states,
//! frames, requests and errors.

use std::fmt;
use std::sync::Arc;

use moho::openusd::sdf;
use moho::openusd::tf::Token;

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

/// One render var's pixels at one point in time: an immutable snapshot, so
/// a reader never locks the renderer and a later snapshot never alters it.
#[derive(Debug, Clone, PartialEq)]
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
    /// The data window's pixels, rows top to bottom, tightly packed,
    /// multisampling already resolved.
    pub pixels: Arc<[u8]>,
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
