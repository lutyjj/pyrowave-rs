//! Regression for descriptor leaks on native import failure.
use std::fs::File;
use std::os::fd::{AsFd, IntoRawFd};

use pyrowave_sys as ffi;

#[test]
#[ignore = "requires a compatible Vulkan GPU"]
fn import_failure_consumes_the_owned_descriptor() {
	let mut device = std::ptr::null_mut();
	// SAFETY: the device output pointer is live for this call.
	assert_eq!(
		unsafe { ffi::pyrowave_create_default_device(&mut device) },
		ffi::pyrowave_result_PYROWAVE_SUCCESS
	);
	let original = File::open("/dev/null").unwrap();
	// Exercise early validation failure and rejection of a valid non-DMA-BUF fd.
	for sharing in [
		ffi::VkSharingMode_VK_SHARING_MODE_CONCURRENT,
		ffi::VkSharingMode_VK_SHARING_MODE_EXCLUSIVE,
	] {
		let owned = original.as_fd().try_clone_to_owned().unwrap().into_raw_fd();
		let layout = ffi::VkSubresourceLayout {
			rowPitch: 256,
			..Default::default()
		};
		let modifier = ffi::VkImageDrmFormatModifierExplicitCreateInfoEXT {
			sType: ffi::VkStructureType_VK_STRUCTURE_TYPE_IMAGE_DRM_FORMAT_MODIFIER_EXPLICIT_CREATE_INFO_EXT,
			drmFormatModifier: 0,
			drmFormatModifierPlaneCount: 1,
			pPlaneLayouts: &layout,
			..Default::default()
		};
		let image_info = ffi::VkImageCreateInfo {
			sType: ffi::VkStructureType_VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
			pNext: (&modifier as *const ffi::VkImageDrmFormatModifierExplicitCreateInfoEXT).cast(),
			imageType: ffi::VkImageType_VK_IMAGE_TYPE_2D,
			format: ffi::VkFormat_VK_FORMAT_R8G8B8A8_UNORM,
			extent: ffi::VkExtent3D {
				width: 64,
				height: 64,
				depth: 1,
			},
			mipLevels: 1,
			arrayLayers: 1,
			samples: ffi::VkSampleCountFlagBits_VK_SAMPLE_COUNT_1_BIT,
			tiling: ffi::VkImageTiling_VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT,
			usage: ffi::VkImageUsageFlagBits_VK_IMAGE_USAGE_SAMPLED_BIT,
			sharingMode: sharing,
			..Default::default()
		};
		let info = ffi::pyrowave_image_create_info {
			device,
			external_handle: owned as ffi::pyrowave_os_handle,
			handle_type: ffi::VkExternalMemoryHandleTypeFlagBits_VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT,
			image_create_info: &image_info,
		};
		let mut image = std::ptr::null_mut();
		// SAFETY: pointers remain live, no GPU operation is submitted. Deliberately
		// invalid import parameters must fail and consume the duplicated fd.
		let result = unsafe { ffi::pyrowave_image_create_owned_fd(&info, &mut image) };
		assert_ne!(result, ffi::pyrowave_result_PYROWAVE_SUCCESS);
		assert!(image.is_null());
		assert!(
			!std::path::Path::new(&format!("/proc/self/fd/{owned}")).exists(),
			"failed import leaked its fd"
		);
		assert!(
			original.metadata().is_ok(),
			"the caller's original descriptor was closed"
		);
	}
	// SAFETY: no image was imported; the device has no surviving resources.
	unsafe { ffi::pyrowave_device_destroy(device) };
}
