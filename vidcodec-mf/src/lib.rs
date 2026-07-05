//! Windows Media Foundation backend for `vidcodec`.

#![deny(missing_docs)]
#[cfg(windows)]
extern crate alloc;

#[cfg(windows)]
mod backend;
#[cfg(windows)]
mod com;
#[cfg(windows)]
mod error;
#[cfg(windows)]
mod h264;
#[cfg(windows)]
mod media_type;
#[cfg(windows)]
mod mft;
#[cfg(windows)]
mod pixel;
#[cfg(windows)]
mod probe;
#[cfg(windows)]
mod profile;
#[cfg(windows)]
mod sample;

#[cfg(windows)]
pub use backend::try_register;

/// Registers the Media Foundation backend when running on Windows.
///
/// # Errors
///
/// On non-Windows targets returns [`vidcodec_core::Error::backend`]. On Windows,
/// propagates COM/MF initialization and probe failures.
#[cfg(not(windows))]
pub fn try_register() -> Result<(), vidcodec_core::Error> {
    Err(vidcodec_core::Error::backend(
        "Media Foundation backend is only available on Windows",
    ))
}
