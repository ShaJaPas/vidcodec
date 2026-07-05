//! HRESULT mapping.

use vidcodec_core::Error;
use windows::Win32::Media::MediaFoundation::MF_E_TRANSFORM_NEED_MORE_INPUT;
use windows::core::HRESULT;

/// Extension trait to convert `Result<T, windows::core::Error>` to `Result<T, vidcodec_core::Error>`.
pub(crate) trait WinResultExt<T> {
    fn mf(self) -> Result<T, Error>;
}

impl<T> WinResultExt<T> for Result<T, windows::core::Error> {
    fn mf(self) -> Result<T, Error> {
        self.map_err(|e| Error::backend(e.to_string()))
    }
}

/// Maps a Win32 [`HRESULT`] to [`Error::backend`].
pub(crate) fn map_hresult(result: HRESULT, context: &str) -> Result<(), Error> {
    result
        .ok()
        .map_err(|err| Error::backend(format!("{context}: {err}")))
}

/// Returns `true` when a transform `ProcessOutput` call needs more input first.
pub(crate) fn is_need_more_input(err: &windows::core::Error) -> bool {
    err.code() == MF_E_TRANSFORM_NEED_MORE_INPUT
}
