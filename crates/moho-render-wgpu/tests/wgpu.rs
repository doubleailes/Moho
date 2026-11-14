//! The wgpu backend on the software adapter (lavapipe, WARP): one test per
//! scenario of the `render-control-wgpu` spec that needs a device. Without
//! an adapter each test says it was skipped, and why, and passes.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use moho_render::{Frame, PixelFormat, PixelRect};
use moho_render_wgpu::{ImageError, WgpuDriver, WgpuImage, wgpu};

/// One instance per binary: creating several and enumerating adapters
/// concurrently crashes some Vulkan loaders.
fn instance() -> &'static wgpu::Instance {
    static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::PRIMARY;
        wgpu::Instance::new(desc)
    })
}

/// A new device on the software adapter, `None` (after saying so) where
/// there is none.
fn device(test: &str) -> Option<WgpuDriver> {
    let adapter = pollster::block_on(instance().request_adapter(&wgpu::RequestAdapterOptions {
        force_fallback_adapter: true,
        ..Default::default()
    }));
    let adapter = match adapter {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("{test}: skipped, no software adapter: {error}");
            return None;
        }
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some(WgpuDriver::new(adapter, device, queue))
}

/// GPU tests of this binary run one at a time (WARP crashes otherwise).
fn lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn texture(driver: &WgpuDriver, format: wgpu::TextureFormat, size: (u32, u32), data: &[u8]) -> wgpu::Texture {
    let texture = driver.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if !data.is_empty() {
        let texel = format.block_copy_size(None).unwrap();
        driver.queue.write_texture(
            texture.as_image_copy(),
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.0 * texel),
                rows_per_image: Some(size.1),
            },
            wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}

fn frame(driver: &WgpuDriver, texture: wgpu::Texture, format: PixelFormat) -> Frame {
    let size = (texture.width(), texture.height());
    let image = WgpuImage::new(driver, texture, format).unwrap();
    let window = PixelRect::of_size(size.0, size.1);
    Frame::on_device(format, window, window, 1, Arc::new(image))
}

#[test]
fn rows_unpadded() {
    let _gpu = lock();
    let Some(driver) = device("rows_unpadded") else {
        return;
    };
    let data: Vec<u8> = (0..24).collect();
    let texture = texture(&driver, wgpu::TextureFormat::Rgba8Unorm, (3, 2), &data);
    let frame = frame(&driver, texture, PixelFormat::U8x4);
    assert_eq!(
        &*frame.read_pixels().unwrap(),
        &data[..],
        "24 bytes, rows top to bottom"
    );
    let pixel = frame
        .read_rect(PixelRect {
            x: 2,
            y: 1,
            width: 1,
            height: 1,
        })
        .unwrap();
    assert_eq!(pixel, [20, 21, 22, 23]);
}

#[test]
fn unsigned_identifiers() {
    let _gpu = lock();
    let Some(driver) = device("unsigned_identifiers") else {
        return;
    };
    let data: Vec<u8> = [0xFFFF_FFFFu32, 7].iter().flat_map(|v| v.to_le_bytes()).collect();
    let texture = texture(&driver, wgpu::TextureFormat::R32Uint, (2, 1), &data);
    let frame = frame(&driver, texture, PixelFormat::I32);
    let pixels = frame.read_pixels().unwrap();
    assert_eq!(i32::from_le_bytes(pixels[0..4].try_into().unwrap()), -1);
    assert_eq!(i32::from_le_bytes(pixels[4..8].try_into().unwrap()), 7);
}

#[test]
fn matching_formats() {
    let _gpu = lock();
    let Some(driver) = device("matching_formats") else {
        return;
    };
    let half_one = 0x3C00u16.to_le_bytes();
    let data: Vec<u8> = (0..4 * 4).flat_map(|_| half_one).collect();
    let texture = texture(&driver, wgpu::TextureFormat::Rgba16Float, (2, 2), &data);
    let frame = frame(&driver, texture, PixelFormat::F16x4);
    let pixels = frame.read_pixels().unwrap();
    assert_eq!(pixels.len(), 2 * 2 * 8, "eight bytes per pixel");
    assert_eq!(&*pixels, &data[..]);
}

#[test]
fn refused_textures() {
    let _gpu = lock();
    let Some(driver) = device("refused_textures") else {
        return;
    };
    let ids = texture(&driver, wgpu::TextureFormat::R32Uint, (1, 1), &[]);
    match WgpuImage::new(&driver, ids, PixelFormat::F32x4) {
        Err(error @ ImageError::FormatMismatch { .. }) => {
            let text = error.to_string();
            assert!(text.contains("R32Uint") && text.contains("float32Vec4"), "{text}");
        }
        other => panic!("expected a format mismatch, got {other:?}"),
    }
    let colour = texture(&driver, wgpu::TextureFormat::Rgba32Float, (1, 1), &[]);
    assert!(matches!(
        WgpuImage::new(&driver, colour, PixelFormat::F32x3),
        Err(ImageError::NoTextureFormat(PixelFormat::F32x3))
    ));
    let sampled_only = driver.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    assert!(matches!(
        WgpuImage::new(&driver, sampled_only, PixelFormat::F32),
        Err(ImageError::NotCopySource)
    ));
}

/// The host copies the frame's texture into one of its own on the driver's
/// queue and reads that back: the renderer's writes are visible to it.
#[test]
fn sampled_by_the_host() {
    let _gpu = lock();
    let Some(driver) = device("sampled_by_the_host") else {
        return;
    };
    let data: Vec<u8> = (0..16 * 4).map(|v| v as u8).collect();
    let rendered = texture(&driver, wgpu::TextureFormat::R32Float, (4, 4), &data);
    let frame = frame(&driver, rendered, PixelFormat::F32);
    let image = WgpuImage::of(&frame).unwrap();
    assert!(image.is_on(&driver), "the renderer drew on the host's device");
    let own = texture(&driver, wgpu::TextureFormat::R32Float, (4, 4), &[]);
    let mut encoder = driver.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_texture(
        image.texture().as_image_copy(),
        own.as_image_copy(),
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    driver.queue.submit([encoder.finish()]);
    let copied = self::frame(&driver, own, PixelFormat::F32);
    assert_eq!(copied.read_pixels().unwrap(), frame.read_pixels().unwrap());
}

#[test]
fn renderer_on_the_hosts_device() {
    let _gpu = lock();
    let Some(host) = device("renderer_on_the_hosts_device") else {
        return;
    };
    let drivers = host.drivers();
    let handed = WgpuDriver::from_drivers(&drivers).unwrap();
    assert!(handed.same_device(&host));
    let texture = texture(handed, wgpu::TextureFormat::R32Float, (1, 1), &[0; 4]);
    let frame = frame(handed, texture, PixelFormat::F32);
    assert!(WgpuImage::of(&frame).unwrap().is_on(&host));
    let Some(other) = device("renderer_on_the_hosts_device") else {
        return;
    };
    assert!(!WgpuImage::of(&frame).unwrap().is_on(&other), "another device");
}

#[test]
fn a_host_frame_is_no_wgpu_image() {
    let window = PixelRect::of_size(1, 1);
    let frame = Frame::new(PixelFormat::F32, window, window, 1, vec![0; 4].into());
    assert!(WgpuImage::of(&frame).is_none());
}

/// Clones share one gate: a submission through one waits while another
/// clone holds it, and so does a readback.
#[test]
fn clones_share_the_submission_gate() {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let _gpu = lock();
    let Some(host) = device("clones_share_the_submission_gate") else {
        return;
    };
    let renderer = WgpuDriver::from_drivers(&host.drivers()).unwrap().clone();
    let texture = texture(&renderer, wgpu::TextureFormat::R32Float, (1, 1), &[0; 4]);
    let frame = frame(&renderer, texture, PixelFormat::F32);
    let (held, release) = (mpsc::channel(), mpsc::channel::<()>());
    let holder = {
        let host = host.clone();
        let held = held.0;
        let release = release.1;
        std::thread::spawn(move || {
            host.exclusive(|| {
                held.send(()).unwrap();
                release.recv().unwrap();
            })
        })
    };
    held.1.recv().unwrap();
    let started = Instant::now();
    let (done, finished) = mpsc::channel();
    let submitter = {
        let renderer = renderer.clone();
        std::thread::spawn(move || {
            let encoder = renderer.device.create_command_encoder(&Default::default());
            renderer.submit([encoder.finish()]);
            let pixels = frame.read_pixels().unwrap();
            done.send(pixels.len()).unwrap();
        })
    };
    assert!(
        finished.recv_timeout(Duration::from_millis(200)).is_err(),
        "the submission waits while another clone holds the gate"
    );
    release.0.send(()).unwrap();
    assert_eq!(finished.recv_timeout(Duration::from_secs(20)).unwrap(), 4);
    assert!(started.elapsed() >= Duration::from_millis(200));
    holder.join().unwrap();
    submitter.join().unwrap();
}
