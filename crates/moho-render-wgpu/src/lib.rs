//! The wgpu backend of Moho's render control, as HgiWgpu backs Hgi.
//!
//! A host creates its wgpu device and hands it to a renderer plugin as the
//! `renderDriver` ([`WgpuDriver::drivers`]). The renderer draws into
//! textures of that device, submits on its queue, and publishes them as
//! device frames ([`WgpuImage`]). A host that knows wgpu reaches the texture
//! ([`WgpuImage::of`]) and uses it with no copy; any host reads the frame
//! back through render control ([`Frame::read_pixels`]).
//!
//! **Synchronization.** A renderer submits its writes to a texture before it
//! publishes the frame, and wgpu orders the submissions of one queue: a
//! command the host submits on the driver's queue after reading the frame
//! sees the writes. A published texture is never written again.
//!
//! **One wgpu.** This crate is the only one of the stack naming wgpu; it
//! re-exports it ([`wgpu`]) so a renderer's textures and a host's are of one
//! type, as `moho::openusd` is the stack's openusd.

#![forbid(unsafe_code)]

use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex, mpsc};

use moho::openusd::tf::Token;
use moho_render::tokens::driver::RENDER_DRIVER;
use moho_render::{DeviceImage, Drivers, Frame, PixelFormat, PixelRect, ReadbackError};

pub use wgpu;

/// The name of this backend, as [`DeviceImage::backend`] gives it.
pub const BACKEND: &str = "wgpu";

/// The device a host created, handed to a renderer as its `renderDriver`:
/// a renderer creates its textures on [`device`](Self::device) and submits
/// on [`queue`](Self::queue).
#[derive(Debug, Clone)]
pub struct WgpuDriver {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// Submissions, presents and surface reconfigurations on the device
    /// exclude each other, for every holder of a clone: a reconfiguration
    /// racing a submission can fail inside wgpu's idle wait.
    gate: Arc<Mutex<()>>,
    /// Serializes readbacks' blocking polls: some drivers (WARP) deadlock on
    /// concurrent `poll(Wait)` of one device.
    readback: Arc<Mutex<()>>,
}

impl WgpuDriver {
    /// The driver of the device and queue a host created on `adapter`.
    pub fn new(adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            adapter,
            device,
            queue,
            gate: Arc::default(),
            readback: Arc::default(),
        }
    }

    /// Runs `f` under the device's submission gate: no submission, present
    /// or surface reconfiguration through any clone overlaps it.
    pub fn exclusive<R>(&self, f: impl FnOnce() -> R) -> R {
        let _gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f()
    }

    /// Submits `commands` on the queue under the gate.
    pub fn submit(&self, commands: impl IntoIterator<Item = wgpu::CommandBuffer>) -> wgpu::SubmissionIndex {
        self.exclusive(|| self.queue.submit(commands))
    }

    /// Presents a surface image under the gate (presenting may submit).
    pub fn present(&self, image: wgpu::SurfaceTexture) {
        self.exclusive(|| self.queue.present(image));
    }

    /// Reconfigures `surface` for this device under the gate. Error scopes
    /// are the caller's: push one around the call to catch a validation
    /// error.
    pub fn configure_surface(&self, surface: &wgpu::Surface<'_>, config: &wgpu::SurfaceConfiguration) {
        self.exclusive(|| surface.configure(&self.device, config));
    }

    /// Render control drivers holding this one as the `renderDriver`.
    pub fn drivers(&self) -> Drivers {
        Drivers::new().with(RENDER_DRIVER, self.clone())
    }

    /// The `renderDriver` of `drivers`, when it is a wgpu driver.
    pub fn from_drivers(drivers: &Drivers) -> Option<&WgpuDriver> {
        drivers.get::<WgpuDriver>(&Token::new(RENDER_DRIVER))
    }

    /// Whether `other` drives the same device.
    pub fn same_device(&self, other: &WgpuDriver) -> bool {
        self.device == other.device
    }
}

/// The wgpu texture formats holding a pixel format's bytes unchanged, the
/// preferred first; none for a format wgpu has no texture for (three
/// floats).
pub fn texture_formats(format: PixelFormat) -> &'static [wgpu::TextureFormat] {
    use wgpu::TextureFormat as T;
    match format {
        PixelFormat::F16x4 => &[T::Rgba16Float],
        PixelFormat::F32x4 => &[T::Rgba32Float],
        PixelFormat::F32 => &[T::R32Float],
        PixelFormat::I32 => &[T::R32Sint, T::R32Uint],
        PixelFormat::U8x4 => &[T::Rgba8Unorm, T::Rgba8UnormSrgb],
        PixelFormat::F32x3 => &[],
    }
}

/// Why a texture cannot be a frame's device image.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageError {
    /// wgpu has no texture format for the pixel format.
    NoTextureFormat(PixelFormat),
    /// The texture's format does not hold the frame's pixel format.
    FormatMismatch {
        frame: PixelFormat,
        texture: wgpu::TextureFormat,
    },
    /// The texture cannot be copied from, so it cannot be read back.
    NotCopySource,
    /// The texture is not of the driver's device.
    OtherDevice,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::NoTextureFormat(format) => {
                write!(f, "wgpu has no texture format for {}", format.token().as_str())
            }
            ImageError::FormatMismatch { frame, texture } => write!(
                f,
                "a {texture:?} texture does not hold {} pixels",
                frame.token().as_str()
            ),
            ImageError::NotCopySource => {
                f.write_str("the texture lacks COPY_SRC usage: it cannot be read back")
            }
            ImageError::OtherDevice => f.write_str("the texture is not of the driver's device"),
        }
    }
}

impl std::error::Error for ImageError {}

/// A frame's pixels in a texture of a [`WgpuDriver`]'s device.
pub struct WgpuImage {
    backend: Token,
    driver: WgpuDriver,
    texture: wgpu::Texture,
    format: PixelFormat,
}

impl WgpuImage {
    /// The image of `texture`, of `driver`'s device, holding `format`
    /// pixels. Refused when the texture's format does not hold them, when
    /// it cannot be copied from, or when wgpu has no format for them.
    pub fn new(driver: &WgpuDriver, texture: wgpu::Texture, format: PixelFormat) -> Result<Self, ImageError> {
        let formats = texture_formats(format);
        if formats.is_empty() {
            return Err(ImageError::NoTextureFormat(format));
        }
        if !formats.contains(&texture.format()) {
            return Err(ImageError::FormatMismatch {
                frame: format,
                texture: texture.format(),
            });
        }
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(ImageError::NotCopySource);
        }
        Ok(Self {
            backend: Token::new(BACKEND),
            driver: driver.clone(),
            texture,
            format,
        })
    }

    /// The wgpu image of `frame`, `None` for a host frame or another
    /// backend's image.
    pub fn of(frame: &Frame) -> Option<&WgpuImage> {
        frame.device_image()?.as_any().downcast_ref::<WgpuImage>()
    }

    /// The texture: sample or copy it on the driver's queue.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The driver whose device holds the texture.
    pub fn driver(&self) -> &WgpuDriver {
        &self.driver
    }

    /// Whether the texture is of `driver`'s device, so a host can use it
    /// without reading it back.
    pub fn is_on(&self, driver: &WgpuDriver) -> bool {
        self.driver.same_device(driver)
    }

    fn read_region(&self, rect: PixelRect) -> Result<Vec<u8>, String> {
        let device = &self.driver.device;
        let texel = self.format.bytes_per_pixel() as u32;
        let row = rect.width.checked_mul(texel).ok_or("readback row overflow")?;
        let padded = row
            .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            .checked_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            .ok_or("readback row overflow")?;
        let size = u64::from(padded) * u64::from(rect.height);
        if size == 0 {
            return Ok(Vec::new());
        }
        if size > device.limits().max_buffer_size {
            return Err("readback exceeds the device's buffer limit".into());
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("moho-render-wgpu readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("moho-render-wgpu readback"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: rect.x.max(0) as u32,
                    y: rect.y.max(0) as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(rect.height),
                },
            },
            wgpu::Extent3d {
                width: rect.width,
                height: rect.height,
                depth_or_array_layers: 1,
            },
        );
        let _polling = self
            .driver
            .readback
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let submission = self.driver.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (done, complete) = mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = done.send(result);
        });
        let result = (|| {
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: None,
                })
                .map_err(|e| e.to_string())?;
            complete
                .recv()
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let mapped = slice.get_mapped_range().map_err(|e| e.to_string())?;
            let mut bytes = Vec::with_capacity(row as usize * rect.height as usize);
            for y in 0..rect.height as usize {
                let start = y * padded as usize;
                bytes.extend_from_slice(mapped.get(start..start + row as usize).ok_or("short mapping")?);
            }
            Ok(bytes)
        })();
        buffer.unmap();
        result
    }
}

impl DeviceImage for WgpuImage {
    fn backend(&self) -> &Token {
        &self.backend
    }

    fn format(&self) -> PixelFormat {
        self.format
    }

    fn size(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }

    fn read(&self, rect: PixelRect) -> Result<Vec<u8>, ReadbackError> {
        self.read_region(rect).map_err(ReadbackError::Device)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl fmt::Debug for WgpuImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WgpuImage")
            .field("format", &self.format)
            .field("texture", &self.texture.format())
            .field("size", &self.size())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_but_three_floats_has_a_texture() {
        for format in [
            PixelFormat::F16x4,
            PixelFormat::F32x4,
            PixelFormat::F32,
            PixelFormat::I32,
            PixelFormat::U8x4,
        ] {
            let formats = texture_formats(format);
            assert!(!formats.is_empty(), "{format:?}");
            for texture in formats {
                assert_eq!(
                    texture.block_copy_size(None),
                    Some(format.bytes_per_pixel() as u32),
                    "{format:?} as {texture:?} keeps its bytes"
                );
            }
        }
        assert!(texture_formats(PixelFormat::F32x3).is_empty());
    }

    #[test]
    fn errors_name_their_formats() {
        let error = ImageError::FormatMismatch {
            frame: PixelFormat::F32x4,
            texture: wgpu::TextureFormat::R32Uint,
        }
        .to_string();
        assert!(
            error.contains("R32Uint") && error.contains("float32Vec4"),
            "{error}"
        );
        let error = ImageError::NoTextureFormat(PixelFormat::F32x3).to_string();
        assert!(error.contains("float32Vec3"), "{error}");
    }
}
