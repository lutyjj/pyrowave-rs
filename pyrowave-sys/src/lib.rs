//! Raw bindings to the vendored PyroWave C API.
//!
//! PyroWave is Hans-Kristian Arntzen's intra-only wavelet codec, run as Vulkan
//! compute. The `pyrowave` crate owns Rust resource lifetimes and provides
//! `with_native` for synchronizing raw calls with its ownership wrappers.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, improper_ctypes)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// Full revision recorded in `UPSTREAM_REVISIONS`.
pub const UPSTREAM_REVISION: &str = env!("PYROWAVE_UPSTREAM_REVISION");

/// First eight hex digits of the native source revision, identifying the implementation.
pub const BITSTREAM_ID: &str = env!("PYROWAVE_BITSTREAM_ID");

#[cfg(test)]
mod tests {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/export_test.rs"));

	#[test]
	fn api_version_matches_vendored_pin() {
		let (mut major, mut minor, mut patch) = (0u32, 0u32, 0u32);
		// SAFETY: writes three u32s through live out-pointers; touches no device state.
		unsafe { pyrowave_get_api_version(&mut major, &mut minor, &mut patch) };
		assert_eq!(
			(major, minor, patch),
			(
				PYROWAVE_API_VERSION_MAJOR,
				PYROWAVE_API_VERSION_MINOR,
				PYROWAVE_API_VERSION_PATCH
			)
		);
	}
}
