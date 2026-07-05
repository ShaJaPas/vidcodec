//! Minimal raw FFI supplement for the `mediacodec` crate.
//!
//! The `mediacodec` crate does **not** expose `AMediaFormat_setBuffer` through
//! its safe API, yet we need it to pass SPS/PPS as `csd-0`/`csd-1` to the
//! H.264 decoder.  Since the crate already links `libmediandk`, we just
//! declare the one function we are missing here.

use core::ffi::{c_char, c_void};

/// Opaque AMediaFormat handle (matches the mediacodec crate's internal type).
#[repr(C)]
pub enum AMediaFormat {}

extern "C" {
    /// Available since API 21.
    pub fn AMediaFormat_setBuffer(
        format: *mut AMediaFormat,
        name: *const c_char,
        value: *const c_void,
        size: usize,
    ) -> bool;
}
