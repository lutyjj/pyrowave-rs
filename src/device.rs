use std::ffi::{CStr, c_char, c_void};
use std::rc::Rc;
use std::sync::{PoisonError, RwLock};

use pyrowave_sys as ffi;

use crate::error::check;
use crate::{Error, Result};

/// Queue family the device submits encode and decode work to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueType {
	Graphics,
	/// Async compute, which upstream suggests for encoding outside the producing
	/// process because the graphics queue may become busy. Upstream uses the
	/// graphics queue when the device has no async compute queue.
	Compute,
}

/// Vulkan global priority of the device's compute queue, from the levels
/// upstream requests: `Medium` by default, `High` or `Realtime` on request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GlobalPriority {
	Medium,
	High,
	Realtime,
}

impl GlobalPriority {
	fn native(self) -> ffi::VkQueueGlobalPriority {
		match self {
			Self::Medium => ffi::VkQueueGlobalPriority_VK_QUEUE_GLOBAL_PRIORITY_MEDIUM,
			Self::High => ffi::VkQueueGlobalPriority_VK_QUEUE_GLOBAL_PRIORITY_HIGH,
			Self::Realtime => ffi::VkQueueGlobalPriority_VK_QUEUE_GLOBAL_PRIORITY_REALTIME,
		}
	}
}

/// A private Vulkan device.
///
/// Clones share ownership. Codecs and images retain the device until they drop.
/// This type is neither `Send` nor `Sync`: the native device needs external
/// synchronization, so each device and its resources stay on one thread.
/// Creating or destroying a device excludes all other native calls in the
/// process. Other calls on separate devices may run concurrently. Calls through
/// `pyrowave-sys` must use [`with_native`] when mixed with this crate.
#[derive(Clone)]
pub struct Device {
	pub(crate) inner: Rc<DeviceInner>,
}

pub(crate) struct DeviceInner {
	pub handle: ffi::pyrowave_device,
}

impl Device {
	/// Create upstream's default device: the graphics queue at medium priority.
	pub fn new() -> Result<Self> {
		let mut handle = std::ptr::null_mut();
		// SAFETY: the output pointer is live for the call.
		check(
			with_native(|| unsafe { ffi::pyrowave_create_default_device(&mut handle) }),
			"creating device",
		)?;
		Ok(Self {
			inner: Rc::new(DeviceInner { handle }),
		})
	}

	/// Create the default device, requesting a global priority for its compute queue.
	///
	/// `High` and `Realtime` also select [`QueueType::Compute`]. Upstream falls
	/// back towards `Medium` when the system refuses a level: on Linux, levels
	/// above `Medium` need root or `CAP_SYS_NICE`. [`Self::global_priority`]
	/// reports the granted level.
	pub fn with_global_priority(priority: GlobalPriority) -> Result<Self> {
		let mut handle = std::ptr::null_mut();
		// SAFETY: null identifiers select the default GPU; the output pointer is live.
		check(
			with_native(|| unsafe {
				ffi::pyrowave_create_device_by_compat(
					0,
					0,
					std::ptr::null(),
					std::ptr::null(),
					std::ptr::null(),
					priority.native(),
					&mut handle,
				)
			}),
			"creating device",
		)?;
		Ok(Self {
			inner: Rc::new(DeviceInner { handle }),
		})
	}

	/// Global priority granted to the compute queue.
	pub fn global_priority(&self) -> Result<GlobalPriority> {
		// SAFETY: the device is live; this query does not submit work.
		let priority = with_native_shared(|| unsafe { ffi::pyrowave_device_get_global_priority(self.as_raw()) });
		[GlobalPriority::Medium, GlobalPriority::High, GlobalPriority::Realtime]
			.into_iter()
			.find(|level| level.native() == priority)
			.ok_or(Error::InvalidOutput(
				"native device reported an unknown global priority",
			))
	}

	/// Select the queue family for subsequent encode and decode submissions.
	pub fn set_queue_type(&self, queue: QueueType) -> Result<()> {
		let flags = match queue {
			QueueType::Graphics => ffi::VkQueueFlagBits_VK_QUEUE_GRAPHICS_BIT,
			QueueType::Compute => ffi::VkQueueFlagBits_VK_QUEUE_COMPUTE_BIT,
		};
		// SAFETY: the device is live and confined to this thread.
		check(
			with_native_shared(|| unsafe { ffi::pyrowave_device_set_queue_type(self.as_raw(), flags) }),
			"selecting queue type",
		)
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
		with_native_shared(|| unsafe {
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
		with_native(|| unsafe { ffi::pyrowave_device_destroy(self.handle) });
	}
}

/// Device creation and destruction take this exclusively; other native calls
/// share it, so work on separate devices is not serialized. Creation rewrites
/// the backend's process-wide Vulkan dispatch state.
///
/// Destruction only reads that state. It takes the lock exclusively as a
/// workaround for a driver defect: on NVIDIA 615.71.09, destroying separate
/// devices concurrently hung inside `vkDestroyDevice` in 5 of 15 runs of
/// `devices_on_separate_threads_create_encode_and_drop_concurrently`. Vulkan
/// allows it and upstream does not serialize it. Relax this only after that
/// test passes many consecutive runs with destruction shared.
static NATIVE: RwLock<()> = RwLock::new(());

/// Run raw native calls excluding every native call made by this crate.
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
	let _guard = NATIVE.write().unwrap_or_else(PoisonError::into_inner);
	call()
}

/// Run a native call that neither creates nor destroys a device, concurrently
/// with other such calls. Creation and destruction must use [`with_native`].
pub(crate) fn with_native_shared<T>(call: impl FnOnce() -> T) -> T {
	let _guard = NATIVE.read().unwrap_or_else(PoisonError::into_inner);
	call()
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::{AtomicBool, Ordering};
	use std::sync::mpsc;
	use std::thread;
	use std::time::Duration;

	use super::{with_native, with_native_shared};

	const WAIT: Duration = Duration::from_secs(5);

	// One test, because concurrent tests would contend for the same process-wide lock.
	#[test]
	fn shared_calls_overlap_and_raw_calls_exclude_them() {
		let (entered, inside) = mpsc::channel();
		let (finish, finished) = mpsc::channel::<()>();
		let holder = thread::spawn(move || {
			with_native_shared(|| {
				entered.send(()).unwrap();
				finished.recv_timeout(WAIT).expect("the second shared call never ran");
			})
		});
		inside.recv_timeout(WAIT).unwrap();
		// Blocks until the holder's timeout if shared calls exclude each other.
		with_native_shared(|| finish.send(()).unwrap());
		holder.join().unwrap();

		let (entered, inside) = mpsc::channel();
		let released = AtomicBool::new(false);
		thread::scope(|scope| {
			scope.spawn(|| {
				with_native(|| {
					entered.send(()).unwrap();
					// Widens the window in which a shared call could wrongly run here.
					thread::sleep(Duration::from_millis(50));
					released.store(true, Ordering::SeqCst);
				})
			});
			inside.recv_timeout(WAIT).unwrap();
			with_native_shared(|| assert!(released.load(Ordering::SeqCst), "a shared call ran inside with_native"));
		});
	}
}
