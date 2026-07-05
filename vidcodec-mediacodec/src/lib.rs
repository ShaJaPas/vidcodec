//! Android MediaCodec backend for `vidcodec`.

#![deny(missing_docs)]

#[cfg(target_os = "android")]
mod backend;
#[cfg(target_os = "android")]
mod error;
#[cfg(target_os = "android")]
mod h264;
#[cfg(target_os = "android")]
mod pixel;
#[cfg(target_os = "android")]
mod probe;
#[cfg(target_os = "android")]
mod profile;

#[cfg(target_os = "android")]
pub use backend::try_register;

/// Registers the MediaCodec backend when running on Android.
///
/// # Errors
///
/// On non-Android targets returns [`vidcodec_core::Error::backend`].  On Android,
/// propagates MediaCodec probe failures.
#[cfg(not(target_os = "android"))]
pub fn try_register() -> Result<(), vidcodec_core::Error> {
    Err(vidcodec_core::Error::backend(
        "MediaCodec backend is only available on Android",
    ))
}
