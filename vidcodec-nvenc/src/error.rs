//! CUDA / NVENC / NVDEC error mapping.

use cudarc::driver::sys::CUresult;
use nvidia_video_codec_sdk::safe::EncodeError;
use vidcodec_core::Error;

/// Maps [`EncodeError`] to [`Error::backend`].
pub(crate) fn map_encode(err: EncodeError) -> Error {
    Error::backend(err.to_string())
}

/// Maps a CUDA driver result to [`Error::backend`].
pub(crate) fn map_cuda(result: CUresult, context: &str) -> Result<(), Error> {
    if result == CUresult::CUDA_SUCCESS {
        Ok(())
    } else {
        Err(Error::backend(format!("{context}: CUDA error {result:?}")))
    }
}
