use pyrowave_sys as ffi;

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromaSampling {
	Yuv420,
	Yuv444,
}

impl ChromaSampling {
	pub(crate) fn native(self) -> ffi::pyrowave_chroma_subsampling {
		match self {
			Self::Yuv420 => ffi::pyrowave_chroma_subsampling_PYROWAVE_CHROMA_SUBSAMPLING_420,
			Self::Yuv444 => ffi::pyrowave_chroma_subsampling_PYROWAVE_CHROMA_SUBSAMPLING_444,
		}
	}

	pub(crate) fn cpu_format(self) -> ffi::pyrowave_cpu_buffer_format {
		match self {
			Self::Yuv420 => ffi::pyrowave_cpu_buffer_format_PYROWAVE_CPU_BUFFER_FORMAT_YUV420P,
			Self::Yuv444 => ffi::pyrowave_cpu_buffer_format_PYROWAVE_CPU_BUFFER_FORMAT_YUV444P,
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoFormat {
	pub width: u32,
	pub height: u32,
	pub chroma: ChromaSampling,
}

impl VideoFormat {
	pub(crate) fn validate(self) -> Result<()> {
		// Width and height minus one each occupy 14 bits in the native sequence header.
		if self.width == 0 || self.height == 0 || self.width > 16384 || self.height > 16384 {
			return Err(Error::InvalidInput("dimensions must be between 1 and 16384"));
		}
		if self.chroma == ChromaSampling::Yuv420 && (!self.width.is_multiple_of(2) || !self.height.is_multiple_of(2)) {
			return Err(Error::InvalidInput("4:2:0 dimensions must be even"));
		}
		Ok(())
	}

	pub(crate) fn plane_layout(self, plane: usize) -> (usize, usize) {
		let divisor = if plane != 0 && self.chroma == ChromaSampling::Yuv420 {
			2
		} else {
			1
		};
		let stride = self.width as usize / divisor;
		(stride, stride * (self.height as usize / divisor))
	}
}
