use std::ops::RangeInclusive;

use pyrowave_sys as ffi;

use crate::{ChromaSampling, Error, Result, VideoFormat};

/// Quality scores the model covers, in dB.
pub const QUALITY_RANGE: RangeInclusive<u32> =
	ffi::PYROWAVE_REGRESSION_MIN_PSNR_HVS_M_H..=ffi::PYROWAVE_REGRESSION_MAX_PSNR_HVS_M_H;

/// A perceived quality level for [`estimate_bitrate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualityTarget {
	/// PSNR-HVS-M score weighted for the viewing distance, in [`QUALITY_RANGE`].
	/// Upstream calls about 35 a good default.
	pub psnr_hvs_m_h: u32,
	/// Distance to the screen in screen heights. The model covers 1.0 to
	/// 2.875 and snaps to steps of 0.125.
	pub viewing_distance: f32,
}

/// Bitrate in bits per second that upstream's regression model estimates
/// for `target`, from its evaluation of difficult game footage.
///
/// The model covers 16:9 formats from 1280x720 to 3840x2160.
pub fn estimate_bitrate(format: VideoFormat, fps: f64, target: QualityTarget) -> Result<f64> {
	format.validate()?;
	if !fps.is_finite() || fps <= 0.0 {
		return Err(Error::InvalidInput("frame rate must be finite and positive"));
	}
	if !(1.0..=2.875).contains(&target.viewing_distance) {
		return Err(Error::InvalidInput("viewing distance is outside the bitrate model"));
	}
	let pixels = format.width as u64 * format.height as u64;
	if !(ffi::PYROWAVE_REGRESSION_MIN_PIXELS as u64..=ffi::PYROWAVE_REGRESSION_MAX_PIXELS as u64).contains(&pixels) {
		return Err(Error::InvalidInput("the bitrate model covers 1280x720 to 3840x2160"));
	}
	if !QUALITY_RANGE.contains(&target.psnr_hvs_m_h) {
		return Err(Error::InvalidInput("quality target is outside the bitrate model"));
	}
	let last_factor = ffi::pyrowave_height_factor_PYROWAVE_HEIGHT_FACTOR_2_87 as f32;
	let height_factor = ((target.viewing_distance - 1.0) / 0.125)
		.round()
		.clamp(0.0, last_factor);
	// SAFETY: a pure lookup; every argument is within the tables' range.
	let mbits = unsafe {
		ffi::pyrowave_quality_estimate_mbits(
			target.psnr_hvs_m_h as i32,
			format.width as i32,
			format.height as i32,
			height_factor as i32,
			(format.chroma == ChromaSampling::Yuv444).into(),
			fps,
		)
	};
	let bitrate = mbits * 1e6;
	if !bitrate.is_finite() {
		return Err(Error::InvalidInput("estimated bitrate exceeds the numeric range"));
	}
	Ok(bitrate)
}
