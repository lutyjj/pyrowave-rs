use pyrowave_sys as ffi;

use crate::error::check;
use crate::{Device, Error, Result, VideoFormat};

/// A native decoder accepting codec records, independent of network framing.
pub struct Decoder {
	handle: ffi::pyrowave_decoder,
	_device: Device,
	format: VideoFormat,
	packet_words: Vec<u32>,
}

impl Decoder {
	pub fn new(device: Device, format: VideoFormat) -> Result<Self> {
		format.validate()?;
		// SAFETY: the device is live; this query does not submit work.
		let fragment_path = crate::device::with_native(|| unsafe {
			ffi::pyrowave_decoder_device_prefers_fragment_path(device.handle())
		});
		let info = ffi::pyrowave_decoder_create_info {
			device: device.handle(),
			width: format.width as i32,
			height: format.height as i32,
			chroma: format.chroma.native(),
			fragment_path,
		};
		let mut handle = std::ptr::null_mut();
		// SAFETY: the device and both pointers remain live for the call.
		check(
			crate::device::with_native(|| unsafe { ffi::pyrowave_decoder_create(&info, &mut handle) }),
			"creating decoder",
		)?;
		Ok(Self {
			handle,
			_device: device,
			format,
			packet_words: Vec::new(),
		})
	}

	/// Discard all queued records.
	pub fn clear(&mut self) {
		// SAFETY: the decoder remains live and access is confined to this thread.
		crate::device::with_native(|| unsafe { ffi::pyrowave_decoder_clear(self.handle) });
	}

	/// Queue one native packet or an entire contiguous encoded frame.
	pub fn push_packet(&mut self, packet: &[u8]) -> Result<()> {
		if packet.is_empty() || !packet.len().is_multiple_of(4) {
			return Err(Error::InvalidInput("codec packets must contain whole 32-bit words"));
		}
		// The C++ parser reads uint32_t values. A byte slice can be unaligned, so
		// retain a reusable aligned copy at this FFI boundary.
		self.packet_words.resize(packet.len() / 4, 0);
		// SAFETY: the destination has packet.len() bytes, is aligned for u32 and
		// cannot overlap the caller's input. The native parser copies queued data.
		crate::device::with_native(|| unsafe {
			std::ptr::copy_nonoverlapping(packet.as_ptr(), self.packet_words.as_mut_ptr().cast(), packet.len());
			check(
				ffi::pyrowave_decoder_push_packet(self.handle, self.packet_words.as_ptr().cast(), packet.len()),
				"queuing codec packet",
			)
		})
	}

	pub fn is_ready(&self, allow_partial_frame: bool) -> bool {
		// SAFETY: the decoder remains live and access is confined to this thread.
		crate::device::with_native(|| unsafe {
			ffi::pyrowave_decoder_decode_is_ready(self.handle, allow_partial_frame)
		})
	}

	/// Decode into tightly packed, planar eight-bit Y, Cb and Cr output.
	///
	/// Waits for GPU completion. Use `pyrowave-sys` for native GPU output and
	/// higher precision; this method follows the upstream CPU readback path.
	pub fn decode_cpu(&mut self, planes: [&mut [u8]; 3]) -> Result<()> {
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
					"output plane is smaller than the configured dimensions",
				));
			}
			buffers.data[index] = plane.as_mut_ptr().cast();
			buffers.row_stride_in_bytes[index] = stride;
			buffers.plane_size_in_bytes[index] = size;
		}
		// SAFETY: each writable plane covers its layout, the buffers are disjoint,
		// and the native implementation completes the readback before returning.
		check(
			crate::device::with_native(|| unsafe {
				ffi::pyrowave_decoder_decode_cpu_buffer_synchronous(self.handle, &buffers)
			}),
			"decoding CPU planes",
		)
	}
}

impl Drop for Decoder {
	fn drop(&mut self) {
		// SAFETY: the device remains retained until the native decoder is destroyed.
		crate::device::with_native(|| unsafe { ffi::pyrowave_decoder_destroy(self.handle) });
	}
}
