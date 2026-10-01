//! Rust ownership for the official PyroWave Vulkan codec.
//!
//! [`Device`], [`Encoder`], [`Decoder`] and imported images keep their native
//! resources alive and stay on one thread. Encoding completes before returning.
//! Applications own capture synchronization, bitrate policy and transport.

mod decoder;
mod device;
mod encoder;
mod error;
mod format;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
mod image;

pub use decoder::Decoder;
pub use device::Device;
pub use encoder::{Encoder, InputColorSpace, IntermediatePrecision, OutputColorSpace, ScaleConfig};
pub use error::{Error, Result};
pub use format::{ChromaSampling, VideoFormat};
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
pub use image::{DmabufDescriptor, ImportedImage, RgbFormat};

/// Pinned native revision. PyroWave's bitstream does not contain a version field.
pub const UPSTREAM_REVISION: &str = pyrowave_sys::UPSTREAM_REVISION;

/// First eight hex digits of [`UPSTREAM_REVISION`], for peer compatibility checks.
pub const BITSTREAM_ID: &str = pyrowave_sys::BITSTREAM_ID;

/// Version reported by the linked C API.
pub fn api_version() -> (u32, u32, u32) {
	let (mut major, mut minor, mut patch) = (0, 0, 0);
	// SAFETY: all three output pointers are live for the call.
	unsafe { pyrowave_sys::pyrowave_get_api_version(&mut major, &mut minor, &mut patch) };
	(major, minor, patch)
}
