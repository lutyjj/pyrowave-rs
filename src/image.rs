use std::os::fd::{BorrowedFd, IntoRawFd};

use pyrowave_sys as ffi;

use crate::error::check;
use crate::{Device, Error, Result};

/// Packed RGB layouts supported by the native scaler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbFormat {
	Rgba8,
	Bgra8,
	Rgb10A2,
	Rgba16Float,
}

impl RgbFormat {
	fn native(self) -> ffi::VkFormat {
		match self {
			Self::Rgba8 => ffi::VkFormat_VK_FORMAT_R8G8B8A8_UNORM,
			Self::Bgra8 => ffi::VkFormat_VK_FORMAT_B8G8R8A8_UNORM,
			Self::Rgb10A2 => ffi::VkFormat_VK_FORMAT_A2B10G10R10_UNORM_PACK32,
			Self::Rgba16Float => ffi::VkFormat_VK_FORMAT_R16G16B16A16_SFLOAT,
		}
	}
}

/// Layout of one packed RGB plane in a DMA-BUF.
#[derive(Clone, Copy, Debug)]
pub struct DmabufDescriptor {
	pub width: u32,
	pub height: u32,
	pub format: RgbFormat,
	pub modifier: u64,
	pub offset: u64,
	pub row_stride: u64,
}

/// An imported image and its cached sampling view, retaining their device.
pub struct ImportedImage {
	pub(crate) handle: ffi::pyrowave_image,
	pub(crate) device: Device,
	pub(crate) view: ffi::pyrowave_image_view,
}

impl Device {
	/// Import a DMA-BUF without taking ownership of the caller's descriptor.
	///
	/// The native boundary consumes a duplicate on both success and failure.
	///
	/// # Safety
	/// `fd` must refer to a DMA-BUF whose allocation matches every field of
	/// `descriptor`. The caller owns synchronization with its producer and must
	/// satisfy [`crate::Encoder::encode_image`]'s contract before encoding it.
	pub unsafe fn import_dmabuf(&self, fd: BorrowedFd<'_>, descriptor: DmabufDescriptor) -> Result<ImportedImage> {
		if descriptor.width == 0 || descriptor.height == 0 || descriptor.row_stride == 0 {
			return Err(Error::InvalidInput("DMA-BUF dimensions and row stride must be nonzero"));
		}
		let layout = ffi::VkSubresourceLayout {
			offset: descriptor.offset,
			rowPitch: descriptor.row_stride,
			..Default::default()
		};
		let modifier = ffi::VkImageDrmFormatModifierExplicitCreateInfoEXT {
			sType: ffi::VkStructureType_VK_STRUCTURE_TYPE_IMAGE_DRM_FORMAT_MODIFIER_EXPLICIT_CREATE_INFO_EXT,
			drmFormatModifier: descriptor.modifier,
			drmFormatModifierPlaneCount: 1,
			pPlaneLayouts: &layout,
			..Default::default()
		};
		let image = ffi::VkImageCreateInfo {
			sType: ffi::VkStructureType_VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
			pNext: (&modifier as *const ffi::VkImageDrmFormatModifierExplicitCreateInfoEXT).cast(),
			imageType: ffi::VkImageType_VK_IMAGE_TYPE_2D,
			format: descriptor.format.native(),
			extent: ffi::VkExtent3D {
				width: descriptor.width,
				height: descriptor.height,
				depth: 1,
			},
			mipLevels: 1,
			arrayLayers: 1,
			samples: ffi::VkSampleCountFlagBits_VK_SAMPLE_COUNT_1_BIT,
			tiling: ffi::VkImageTiling_VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT,
			usage: ffi::VkImageUsageFlagBits_VK_IMAGE_USAGE_SAMPLED_BIT,
			sharingMode: ffi::VkSharingMode_VK_SHARING_MODE_EXCLUSIVE,
			initialLayout: ffi::VkImageLayout_VK_IMAGE_LAYOUT_UNDEFINED,
			..Default::default()
		};
		let info = ffi::pyrowave_image_create_info {
			device: self.handle(),
			external_handle: fd.try_clone_to_owned()?.into_raw_fd() as ffi::pyrowave_os_handle,
			handle_type: ffi::VkExternalMemoryHandleTypeFlagBits_VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT,
			image_create_info: &image,
		};
		let mut handle = std::ptr::null_mut();
		// SAFETY: layout pointers are live during the call; the caller guarantees
		// allocation compatibility. This native extension always consumes the fd.
		check(
			crate::device::with_native(|| unsafe { ffi::pyrowave_image_create_owned_fd(&info, &mut handle) }),
			"importing DMA-BUF",
		)?;
		let mut imported = ImportedImage {
			handle,
			device: self.clone(),
			view: Default::default(),
		};
		// SAFETY: the live image belongs to this retained device and exposes RGB.
		check(
			crate::device::with_native(|| unsafe {
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
}

impl Drop for ImportedImage {
	fn drop(&mut self) {
		// SAFETY: the caller's encoding contract prevents in-flight use at drop;
		// the device remains retained until after this call.
		crate::device::with_native(|| unsafe { ffi::pyrowave_image_destroy(self.handle) });
	}
}
