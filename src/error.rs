use pyrowave_sys as ffi;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("{operation} failed: PyroWave error {code}")]
	Native { operation: &'static str, code: i32 },
	#[error("{0}")]
	InvalidInput(&'static str),
	#[error("{0}")]
	InvalidOutput(&'static str),
	#[error("duplicating DMA-BUF descriptor: {0}")]
	Io(#[from] std::io::Error),
}

pub(crate) fn check(result: ffi::pyrowave_result, operation: &'static str) -> Result<()> {
	if result == ffi::pyrowave_result_PYROWAVE_SUCCESS {
		Ok(())
	} else {
		Err(Error::Native {
			operation,
			code: result,
		})
	}
}
