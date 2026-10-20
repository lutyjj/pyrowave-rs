use pyrowave_sys as ffi;

use crate::error::check;
use crate::{Device, Result};

/// An imported image and its cached sampling view, retaining their device.
pub struct ImportedImage {
	pub(crate) handle: ffi::pyrowave_image,
	pub(crate) device: Device,
	pub(crate) view: ffi::pyrowave_image_view,
}

impl ImportedImage {
	/// Take ownership of an image created through [`crate::sys`].
	///
	/// Ownership transfers even if creating its sampling view fails.
	///
	/// # Safety
	/// `handle` must be a live, non-null, uniquely owned image from the linked library,
	/// created on `device`, with no pending GPU use. Its Vulkan image must support
	/// sampled usage and a COLOR view compatible with the native RGB scaler.
	/// For sRGB formats, allow the corresponding UNORM view through mutable-format
	/// creation and any image format list. Keep its backing allocation
	/// alive through GPU completion and follow upstream synchronization rules.
	pub unsafe fn from_raw(device: Device, handle: ffi::pyrowave_image) -> Result<Self> {
		let mut imported = Self {
			handle,
			device,
			view: Default::default(),
		};
		// SAFETY: the caller guarantees a live image on this retained device.
		check(
			crate::device::with_native_shared(|| unsafe {
				ffi::pyrowave_image_get_image_view(
					handle,
					ffi::VkImageAspectFlagBits_VK_IMAGE_ASPECT_COLOR_BIT,
					ffi::VkImageUsageFlagBits_VK_IMAGE_USAGE_SAMPLED_BIT,
					&mut imported.view,
				)
			}),
			"creating sampling view",
		)?;
		Ok(imported)
	}

	/// Borrow the native imported image without transferring ownership.
	/// Use [`crate::with_native`] for raw calls, retain the image until GPU
	/// completion, and restore external ownership before wrapper encoding.
	pub fn as_raw(&self) -> ffi::pyrowave_image {
		self.handle
	}
}

impl Drop for ImportedImage {
	fn drop(&mut self) {
		// SAFETY: the caller's encoding contract prevents in-flight use at drop;
		// the device remains retained until after this call.
		crate::device::with_native_shared(|| unsafe { ffi::pyrowave_image_destroy(self.handle) });
	}
}
