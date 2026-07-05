//! VA-API status helpers.

use core::ffi::CStr;

use vaapi_sys::{VAStatus, status_ok, vaErrorStr};
use vidcodec_core::Error;

/// Maps a non-success [`VAStatus`] to [`Error::backend`].
pub(crate) fn check(status: VAStatus, context: &str) -> Result<(), Error> {
    if status_ok(status) {
        return Ok(());
    }
    Err(Error::backend(format!("{context}: {}", va_message(status))))
}

/// Returns a human-readable libva message for `status`.
fn va_message(status: VAStatus) -> String {
    // SAFETY: `vaErrorStr` returns a static C string for known status codes.
    // SAFETY: `vaErrorStr` returns a static, null-terminated string valid for the lifetime of the library.
    unsafe {
        let ptr = vaErrorStr(status);
        if ptr.is_null() {
            return format!("VA error {status}");
        }
        CStr::from_ptr(ptr)
            .to_str()
            .unwrap_or("unknown VA error")
            .to_owned()
    }
}
