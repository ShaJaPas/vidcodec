//! NVIDIA NVENC / NVDEC backend for `vidcodec` (Linux and Windows).
//!
//! Requires an NVIDIA GPU, proprietary driver, and (for runtime) the libraries
//! shipped with the driver:
//!
//! - Linux: `libcuda.so.1`, `libnvidia-encode.so.1` (encode), `libnvcuvid.so.1` (decode)
//! - Windows: `nvcuda.dll`, `nvEncodeAPI64.dll`
//!
//! Build with the default `ci-check` bindings when no SDK is installed; enable the
//! `sdk` feature and set `NVIDIA_VIDEO_CODEC_SDK_PATH` for development against a
//! local SDK tree.

#![deny(missing_docs)]

#[cfg(any(target_os = "linux", target_os = "windows"))]
extern crate alloc;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod backend;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod device;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod error;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod frame;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod h264;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod nvdec;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod probe;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod profile;

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub use backend::try_register;

/// Registers the NVENC/NVDEC backend on supported platforms.
///
/// # Errors
///
/// Returns [`vidcodec_core::Error::backend`] on unsupported platforms (e.g. macOS).
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn try_register() -> Result<(), vidcodec_core::Error> {
    Err(vidcodec_core::Error::backend(
        "NVENC/NVDEC is only available on Linux and Windows",
    ))
}
