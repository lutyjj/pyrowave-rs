use std::ffi::{CStr, c_char, c_void};
use std::rc::Rc;

use pyrowave_sys as ffi;

use crate::Result;
use crate::error::check;

/// A private Vulkan device using upstream's default graphics queue.
///
/// Clones share ownership. Codecs and images retain the device until they drop.
/// This type is neither `Send` nor `Sync`: the native device needs external
/// synchronization, so each device and its resources stay on one thread.
/// Native calls from separate devices are serialized because the backend uses
/// process-wide Vulkan dispatch state. Calls through `pyrowave-sys` must also
/// be externally synchronized with this crate.
#[derive(Clone)]
pub struct Device {
	pub(crate) inner: Rc<DeviceInner>,
}

pub(crate) struct DeviceInner {
	pub handle: ffi::pyrowave_device,
}

impl Device {
	pub fn new() -> Result<Self> {
		let mut handle = std::ptr::null_mut();
		// SAFETY: the output pointer is live for the call.
		check(
			crate::device::with_native(|| unsafe { ffi::pyrowave_create_default_device(&mut handle) }),
			"creating device",
		)?;
		Ok(Self {
			inner: Rc::new(DeviceInner { handle }),
		})
	}

	/// GPU time per codec stage and memory heap budgets, one line each.
	/// `reset` starts a new measurement interval.
	pub fn performance_report(&self, reset: bool) -> Vec<String> {
		unsafe extern "C" fn push(lines: *mut c_void, message: *const c_char) {
			// SAFETY: `lines` is the vector passed below; the message is a
			// NUL-terminated string valid for this call.
			unsafe {
				let message = CStr::from_ptr(message).to_string_lossy().into_owned();
				(*lines.cast::<Vec<String>>()).push(message);
			}
		}
		let mut lines = Vec::new();
		// SAFETY: the device is live and the callback only runs during this call.
		crate::device::with_native(|| unsafe {
			ffi::pyrowave_device_report_performance_stats(self.as_raw(), Some(push), (&raw mut lines).cast(), reset);
		});
		lines
	}

	/// Take ownership of a device created through [`crate::sys`].
	///
	/// # Safety
	/// `handle` must be a live, non-null device from the linked native library,
	/// with no other owner that will destroy it. Borrowed Vulkan handles,
	/// creation-info pointers and callback data must outlive this device and
	/// all its clones, as required by upstream's device creation contract.
	pub unsafe fn from_raw(handle: ffi::pyrowave_device) -> Self {
		Self {
			inner: Rc::new(DeviceInner { handle }),
		}
	}

	/// Borrow the native handle without transferring ownership.
	/// Use [`crate::with_native`] for calls through this handle. Do not destroy
	/// it, and clear command-buffer overrides before using wrapper methods.
	pub fn as_raw(&self) -> ffi::pyrowave_device {
		self.inner.handle
	}

	#[cfg(all(feature = "dmabuf", target_os = "linux"))]
	pub(crate) fn same_device(&self, other: &Self) -> bool {
		Rc::ptr_eq(&self.inner, &other.inner)
	}
}

impl Drop for DeviceInner {
	fn drop(&mut self) {
		// SAFETY: all codecs and images have released their strong references.
		crate::device::with_native(|| unsafe { ffi::pyrowave_device_destroy(self.handle) });
	}
}

/// Serialize native calls with all ownership wrappers in this crate.
///
/// The backend shares Vulkan dispatch state across devices. Raw calls that
/// create, use or destroy native resources must run inside this closure when
/// mixed with the wrappers. Calls made only through [`crate::sys`] must follow
/// the same synchronization rule if wrappers are used elsewhere in the process.
///
/// Do not call wrapper methods or nest `with_native` inside the closure: the
/// lock is held until it returns. Native calls remain unsafe; retain borrowed
/// resources until GPU completion and follow their upstream contracts.
pub fn with_native<T>(call: impl FnOnce() -> T) -> T {
	static NATIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
	let _guard = NATIVE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
	call()
}
