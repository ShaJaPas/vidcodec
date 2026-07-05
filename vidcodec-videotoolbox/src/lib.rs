//! Apple VideoToolbox backend for `vidcodec`.

#![deny(missing_docs)]

extern crate alloc;

#[cfg(target_os = "macos")]
mod backend;
#[cfg(target_os = "macos")]
mod encode_opts;
#[cfg(target_os = "macos")]
mod error;
#[cfg(target_os = "macos")]
mod ffi;
#[cfg(target_os = "macos")]
mod h264;
#[cfg(target_os = "macos")]
mod pixel;
#[cfg(target_os = "macos")]
mod probe;
#[cfg(target_os = "macos")]
mod profile;

#[cfg(target_os = "macos")]
pub use backend::try_register;

/// Registers the VideoToolbox backend when running on macOS.
///
/// # Errors
///
/// On non-macOS targets returns [`vidcodec_core::Error::backend`]. On macOS,
/// propagates VideoToolbox probe failures.
#[cfg(not(target_os = "macos"))]
pub fn try_register() -> Result<(), vidcodec_core::Error> {
    Err(vidcodec_core::Error::backend(
        "VideoToolbox backend is only available on macOS",
    ))
}
