//! MediaCodec error mapping.

use ndk::media_error::MediaError;
use vidcodec_core::Error;

/// Maps [`MediaError`] to [`vidcodec_core::Error`].
pub(crate) fn map_media(err: MediaError, context: &str) -> Error {
    let msg = format!("{context}: {err}");
    Error::backend(msg)
}
