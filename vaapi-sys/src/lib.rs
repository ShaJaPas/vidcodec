//! Raw FFI bindings to [VA-API](https://github.com/intel/libva) (`libva`).
//!
//! Bindings are generated at build time via `bindgen` from the installed libva headers.
//! Requires `clang`, `pkg-config`, and libva development packages.
//!
//! # Example
//!
//! ```no_run
//! use vaapi_sys::{vaDisplayIsValid, vaInitialize, vaTerminate, VADisplay};
//!
//! # fn demo(display: VADisplay) -> i32 {
//! unsafe {
//!     if vaDisplayIsValid(display) == 0 {
//!         return -1;
//!     }
//!     let mut major = 0;
//!     let mut minor = 0;
//!     let status = vaInitialize(display, &mut major, &mut minor);
//!     if !vaapi_sys::status_ok(status) {
//!         return status;
//!     }
//!     let _ = vaTerminate(display);
//!     status
//! }
//! # }
//! ```

#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    improper_ctypes,
    dead_code,
    unused_qualifications,
    unsafe_op_in_unsafe_fn,
    clippy::all
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// Returns `true` when `status` is success (`0` / [`VA_STATUS_SUCCESS`]).
#[must_use]
pub const fn status_ok(status: VAStatus) -> bool {
    status == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_constant() {
        assert!(status_ok(0));
        assert!(!status_ok(-1));
    }

    #[test]
    fn h264_profile_constant_exists() {
        assert_eq!(VAProfile_VAProfileH264Main, 6);
    }
}
