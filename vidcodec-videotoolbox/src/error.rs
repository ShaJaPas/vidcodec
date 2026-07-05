//! VideoToolbox / CoreMedia error mapping.

use vidcodec_core::Error;
use videotoolbox::error::VTError;

/// Maps [`VTError`] to [`Error::backend`].
pub(crate) fn map_vt(err: VTError) -> Error {
    Error::backend(err.to_string())
}

/// Maps a CoreMedia / CoreVideo OSStatus to [`Error::backend`].
pub(crate) fn map_osstatus(status: i32, context: &str) -> Result<(), Error> {
    if status == 0 {
        Ok(())
    } else {
        Err(Error::backend(format!("{context}: OSStatus {status}")))
    }
}
