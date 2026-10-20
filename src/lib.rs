//! Rust ownership for the official PyroWave Vulkan codec.
//!
//! [`Device`], [`Encoder`], [`Decoder`] and imported images keep their native
//! resources alive and stay on one thread. Successful encoding waits for readback.
//! Applications own capture synchronization, bitrate policy and transport.
//! [`sys`] exposes the complete native API. Use [`with_native`] to run raw
//! calls that exclude every ownership wrapper call, and keep their resources alive.

mod decoder;
mod device;
mod encoder;
mod error;
mod format;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
mod image;
mod quality;

pub use decoder::Decoder;
pub use device::{Device, GlobalPriority, QueueType, with_native};
pub use encoder::{Encoder, InputColorSpace, IntermediatePrecision, OutputColorSpace, ScaleConfig, YcbcrRange};
pub use error::{Error, Result};
pub use format::{ChromaSampling, VideoFormat};
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
pub use image::ImportedImage;
pub use pyrowave_sys as sys;
pub use quality::{QUALITY_RANGE, QualityTarget, estimate_bitrate};

/// Version of upstream's frozen codec packet format, independent of its C API.
pub const BITSTREAM_VERSION: u32 = 1;

/// Pinned native revision. PyroWave's bitstream does not contain a version field.
pub const UPSTREAM_REVISION: &str = pyrowave_sys::UPSTREAM_REVISION;

/// First eight hex digits of [`UPSTREAM_REVISION`], identifying the implementation.
/// Use [`BITSTREAM_VERSION`] for the codec format; this fingerprint changes on
/// every upstream refresh, including changes that preserve the packet format.
pub const BITSTREAM_ID: &str = pyrowave_sys::BITSTREAM_ID;

/// Version reported by the linked C API.
pub fn api_version() -> (u32, u32, u32) {
	let (mut major, mut minor, mut patch) = (0, 0, 0);
	// SAFETY: all three output pointers are live for the call.
	unsafe { pyrowave_sys::pyrowave_get_api_version(&mut major, &mut minor, &mut patch) };
	(major, minor, patch)
}
