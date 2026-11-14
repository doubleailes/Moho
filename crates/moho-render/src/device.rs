//! Frames that stay on a graphics device, and the drivers a host hands a
//! renderer: render control's counterpart of Hgi's role in Hydra.
//!
//! A host creates its graphics device and hands it to a renderer plugin at
//! construction as a named driver ([`Drivers`], `HdDriverVector`), the
//! device under `renderDriver`. The renderer draws into images of that
//! device and publishes them as device frames
//! ([`FramePixels::Device`](crate::FramePixels)): opaque to render control,
//! typed to a host that names the backend's image type
//! (`HdRenderBuffer::GetResource`), and read back to host memory on demand
//! (`Map`). A backend crate implements [`DeviceImage`] for one graphics API.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use moho::openusd::tf::Token;

use crate::types::{PixelFormat, PixelRect};

/// An image on a graphics device holding one frame's data window. Immutable
/// once its frame is published: a renderer never writes it again.
pub trait DeviceImage: Send + Sync + 'static {
    /// The backend it belongs to (`wgpu`, `test`).
    fn backend(&self) -> &Token;

    /// The pixel format of its texels, as host memory holds them.
    fn format(&self) -> PixelFormat;

    /// Its width and height: the data window's.
    fn size(&self) -> (u32, u32);

    /// The pixels of `rect`, in the image's own coordinates (its top-left
    /// texel at `(0, 0)`), rows top to bottom, tightly packed in
    /// [`format`](Self::format). `rect` lies inside the image: the frame
    /// checks it. Blocks until the transfer is done; callable from any
    /// thread.
    fn read(&self, rect: PixelRect) -> Result<Vec<u8>, ReadbackError>;

    /// The image, for a host that knows its backend's type.
    fn as_any(&self) -> &dyn Any;
}

/// A shared device image.
pub type DeviceImageHandle = Arc<dyn DeviceImage>;

/// Why reading a frame back failed.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadbackError {
    /// The rectangle reaches past the frame's data window.
    OutsideWindow { rect: PixelRect, window: PixelRect },
    /// The device failed the transfer (lost, or the map failed).
    Device(String),
}

impl fmt::Display for ReadbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadbackError::OutsideWindow { rect, window } => write!(
                f,
                "rectangle {}x{} at ({}, {}) reaches past the data window {}x{} at ({}, {})",
                rect.width, rect.height, rect.x, rect.y, window.width, window.height, window.x, window.y
            ),
            ReadbackError::Device(reason) => write!(f, "reading the frame back failed: {reason}"),
        }
    }
}

impl std::error::Error for ReadbackError {}

/// The drivers a host hands a renderer plugin at construction, as
/// `HdDriverVector`: each a name and a value of the host's type. The
/// graphics device the host renders with is named
/// [`RENDER_DRIVER`](crate::tokens::driver::RENDER_DRIVER).
///
/// Moho's renderer create args cannot carry a driver (a data source holds
/// no device), so a plugin takes its drivers in its constructor and answers
/// not supported from `is_supported` when one it needs is missing.
#[derive(Clone, Default)]
pub struct Drivers(Vec<(Token, Arc<dyn Any + Send + Sync>)>);

impl Drivers {
    /// No drivers.
    pub fn new() -> Self {
        Self::default()
    }

    /// These drivers with `driver` named `name`, replacing a driver of that
    /// name.
    pub fn with<T: Any + Send + Sync>(mut self, name: impl Into<Token>, driver: T) -> Self {
        let name = name.into();
        self.0.retain(|(n, _)| *n != name);
        self.0.push((name, Arc::new(driver)));
        self
    }

    /// The driver named `name`, when it is a `T`.
    pub fn get<T: Any + Send + Sync>(&self, name: &Token) -> Option<&T> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, driver)| driver.downcast_ref::<T>())
    }

    /// The drivers' names, in the order given.
    pub fn names(&self) -> impl Iterator<Item = &Token> {
        self.0.iter().map(|(name, _)| name)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Drivers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.names().map(Token::as_str)).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::driver::RENDER_DRIVER;

    #[derive(Debug, PartialEq)]
    struct Device(u32);

    #[test]
    fn a_driver_is_reached_by_name_and_type() {
        let drivers = Drivers::new().with(RENDER_DRIVER, Device(7));
        assert_eq!(
            drivers.get::<Device>(&Token::new(RENDER_DRIVER)),
            Some(&Device(7))
        );
        assert_eq!(drivers.get::<u32>(&Token::new(RENDER_DRIVER)), None, "wrong type");
        assert_eq!(drivers.get::<Device>(&Token::new("other")), None, "wrong name");
    }

    #[test]
    fn an_empty_list_is_valid() {
        let drivers = Drivers::new();
        assert!(drivers.is_empty());
        assert_eq!(drivers.get::<Device>(&Token::new(RENDER_DRIVER)), None);
    }

    #[test]
    fn a_name_given_twice_keeps_the_last() {
        let drivers = Drivers::new()
            .with(RENDER_DRIVER, Device(1))
            .with(RENDER_DRIVER, Device(2));
        assert_eq!(drivers.names().count(), 1);
        assert_eq!(
            drivers.get::<Device>(&Token::new(RENDER_DRIVER)),
            Some(&Device(2))
        );
    }

    #[test]
    fn errors_name_their_subject() {
        let error = ReadbackError::OutsideWindow {
            rect: PixelRect {
                x: 3,
                y: 1,
                width: 2,
                height: 1,
            },
            window: PixelRect::of_size(4, 2),
        }
        .to_string();
        assert!(
            error.contains("2x1 at (3, 1)") && error.contains("4x2"),
            "{error}"
        );
    }
}
