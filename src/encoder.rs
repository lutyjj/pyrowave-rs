use pyrowave_sys as ffi;

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use crate::ImportedImage;
use crate::error::check;
use crate::{Device, Error, Result, VideoFormat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntermediatePrecision {
	Unorm8,
	Unorm16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputColorSpace {
	Srgb,
	Hdr10,
	ScrgbLinear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputColorSpace {
	Srgb,
	Hdr10,
}

/// RGB conversion performed by the native GPU scaler.
///
/// Output is full-range YCbCr: BT.709 for SDR, BT.2020/PQ for HDR10. The
/// scaler uses eight- or sixteen-bit UNORM intermediate planes. This controls
/// conversion precision; PyroWave has no fixed encoded bit depth.
#[derive(Clone, Copy, Debug)]
pub struct ScaleConfig {
	pub input_color_space: InputColorSpace,
	pub output_color_space: OutputColorSpace,
	/// Unorm8 centers Cb/Cr at 128/255; Unorm16 centers them at 0.5.
	/// GPU decoder consumers must use the same chroma midpoint.
	pub intermediate_precision: IntermediatePrecision,
}

/// An encoder retaining its device and reusing its output allocation.
pub struct Encoder {
	handle: ffi::pyrowave_encoder,
	device: Device,
	format: VideoFormat,
	bitstream: Vec<u8>,
}

impl Encoder {
	pub fn new(device: Device, format: VideoFormat) -> Result<Self> {
		format.validate()?;
		let info = ffi::pyrowave_encoder_create_info {
			device: device.handle(),
			width: format.width as i32,
			height: format.height as i32,
			chroma: format.chroma.native(),
		};
		let mut handle = std::ptr::null_mut();
		// SAFETY: the device and both pointers remain live for the call.
		check(
			crate::device::with_native(|| unsafe { ffi::pyrowave_encoder_create(&info, &mut handle) }),
			"creating encoder",
		)?;
		Ok(Self {
			handle,
			device,
			format,
			bitstream: Vec::new(),
		})
	}

	/// Retained device, for creating compatible imported images.
	pub fn device(&self) -> &Device {
		&self.device
	}

	/// Count blocks in the first `bands` wavelet bands (0 through 4).
	pub fn active_blocks(&self, bands: u8) -> Result<usize> {
		let mut count = 0;
		// SAFETY: the encoder is live and the native API validates the band count.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_encoder_get_num_active_blocks(self.handle, bands.into(), &mut count)
			}),
			"counting active blocks",
		)?;
		Ok(count)
	}

	/// Encode tightly packed, planar eight-bit Y, Cb and Cr samples.
	///
	/// `maximum_bytes` is the native rate-control budget for one frame, independent
	/// of network packet size. The returned codec records remain valid until the
	/// next mutable access to the encoder. GPU work completes before this returns.
	pub fn encode_cpu(&mut self, planes: [&[u8]; 3], maximum_bytes: usize) -> Result<&[u8]> {
		let rate = Self::rate_control(maximum_bytes)?;
		let mut buffers = ffi::pyrowave_cpu_buffer {
			width: self.format.width as i32,
			height: self.format.height as i32,
			format: self.format.chroma.cpu_format(),
			..Default::default()
		};
		for (index, plane) in planes.into_iter().enumerate() {
			let (stride, size) = self.format.plane_layout(index);
			if plane.len() < size {
				return Err(Error::InvalidInput(
					"input plane is smaller than the configured dimensions",
				));
			}
			buffers.data[index] = plane.as_ptr().cast_mut().cast();
			buffers.row_stride_in_bytes[index] = stride;
			buffers.plane_size_in_bytes[index] = size;
		}
		// SAFETY: each plane covers its native layout; the native encoder only reads
		// the input and copies it during this call. The device remains live.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_encoder_encode_cpu_synchronous(self.handle, &buffers, &rate)
			}),
			"encoding CPU planes",
		)?;
		self.bitstream()
	}

	/// Encode an imported RGB image with native scaling and color conversion.
	///
	/// The encoder reads the image on the GPU during this call. If the producer
	/// is still writing to it, or writes again before this method returns, the
	/// encoded picture is unspecified. Synchronize with the producer first, for
	/// example by waiting on the DMA-BUF's fences.
	///
	/// # Safety
	/// The producer must have relinquished ownership to `VK_QUEUE_FAMILY_EXTERNAL`
	/// in GENERAL layout. Keep the backing allocation alive until this method
	/// returns, including on error. Submitted work completes and releases
	/// ownership back to the external queue family before returning.
	#[cfg(all(feature = "dmabuf", target_os = "linux"))]
	pub unsafe fn encode_image(
		&mut self,
		image: &ImportedImage,
		scale: ScaleConfig,
		maximum_bytes: usize,
	) -> Result<&[u8]> {
		if !self.device.same_device(&image.device) {
			return Err(Error::InvalidInput("image and encoder belong to different devices"));
		}

		let rate = Self::rate_control(maximum_bytes)?;
		let scaling = ffi::pyrowave_scaled_encode_info {
			view: image.view,
			input_color_space: match scale.input_color_space {
				InputColorSpace::Srgb => ffi::VkColorSpaceKHR_VK_COLOR_SPACE_SRGB_NONLINEAR_KHR,
				InputColorSpace::Hdr10 => ffi::VkColorSpaceKHR_VK_COLOR_SPACE_HDR10_ST2084_EXT,
				InputColorSpace::ScrgbLinear => ffi::VkColorSpaceKHR_VK_COLOR_SPACE_EXTENDED_SRGB_LINEAR_EXT,
			},
			output_color_space: match scale.output_color_space {
				OutputColorSpace::Srgb => ffi::VkColorSpaceKHR_VK_COLOR_SPACE_SRGB_NONLINEAR_KHR,
				OutputColorSpace::Hdr10 => ffi::VkColorSpaceKHR_VK_COLOR_SPACE_HDR10_ST2084_EXT,
			},
			intermediate_plane_format: match scale.intermediate_precision {
				IntermediatePrecision::Unorm8 => ffi::VkFormat_VK_FORMAT_R8_UNORM,
				IntermediatePrecision::Unorm16 => ffi::VkFormat_VK_FORMAT_R16_UNORM,
			},
			ycbcr_chroma_midpoint: match scale.intermediate_precision {
				IntermediatePrecision::Unorm8 => 128.0 / 255.0,
				IntermediatePrecision::Unorm16 => 0.5,
			},
			force_linear_filtering: false,
			skip_dither: false,
			crop_rect: std::ptr::null(),
		};
		let reference = ffi::pyrowave_gpu_external_reference {
			image: image.handle,
			queue_family_index: ffi::VK_QUEUE_FAMILY_EXTERNAL as u32,
		};
		let ownership = ffi::pyrowave_gpu_sync_operation {
			images: &reference,
			num_images: 1,
			sync: ffi::pyrowave_sync_point::default(),
		};
		// SAFETY: the caller supplies the synchronization contract; both resources
		// retain the same device and all pointers remain live during this call.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_encoder_encode_gpu_scaled_synchronous(
					self.handle,
					&ownership,
					&ownership,
					&scaling,
					&rate,
				)
			}),
			"encoding imported image",
		)?;
		self.bitstream()
	}

	fn rate_control(maximum_bytes: usize) -> Result<ffi::pyrowave_rate_control> {
		if maximum_bytes < 8 || maximum_bytes > u32::MAX as usize {
			return Err(Error::InvalidInput("frame budget must be between 8 and u32::MAX bytes"));
		}
		Ok(ffi::pyrowave_rate_control {
			maximum_bitstream_size: maximum_bytes,
		})
	}

	fn bitstream(&mut self) -> Result<&[u8]> {
		let boundary = usize::MAX / 2;
		let mut count = 0;
		// SAFETY: follows a successful encode. This call waits for GPU completion.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_encoder_compute_num_packets(self.handle, boundary, &mut count)
			}),
			"sizing encoded frame",
		)?;
		if count != 1 {
			return Err(Error::InvalidOutput(
				"native encoder did not produce one contiguous frame",
			));
		}

		let (mut data, mut metadata) = (std::ptr::null(), std::ptr::null());
		let (mut data_size, mut metadata_size) = (0, 0);
		// SAFETY: all output pointers are live. Only allocation sizes are used;
		// the native packetizer owns serialization of the mapped codec data.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_encoder_get_mapped_raw_bitstream(
					self.handle,
					&mut data,
					&mut data_size,
					&mut metadata,
					&mut metadata_size,
				)
			}),
			"sizing packetizer output",
		)?;
		// Packed records fit in the source allocation. Metadata size also covers
		// the sequence header; deriving capacity avoids a guessed fixed slack.
		let capacity = data_size
			.checked_add(metadata_size)
			.ok_or(Error::InvalidOutput("native output allocation size overflow"))?;
		self.bitstream.resize(capacity, 0);
		let mut packet = ffi::pyrowave_packet::default();
		let mut written = 0;
		// SAFETY: one descriptor was requested, and the output covers the native
		// allocation plus its metadata. Native serialization runs synchronously.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_encoder_packetize(
					self.handle,
					&mut packet,
					boundary,
					&mut written,
					self.bitstream.as_mut_ptr().cast(),
					self.bitstream.len(),
				)
			}),
			"serializing encoded frame",
		)?;
		let end = packet
			.offset
			.checked_add(packet.size)
			.ok_or(Error::InvalidOutput("native packet range overflow"))?;
		if written != 1 {
			return Err(Error::InvalidOutput("native packetizer did not write one frame"));
		}
		self.bitstream
			.get(packet.offset..end)
			.ok_or(Error::InvalidOutput("native packet exceeds output allocation"))
	}
}

impl Drop for Encoder {
	fn drop(&mut self) {
		// SAFETY: the device is retained until after the native encoder is destroyed.
		crate::device::with_native(|| unsafe { ffi::pyrowave_encoder_destroy(self.handle) });
	}
}
