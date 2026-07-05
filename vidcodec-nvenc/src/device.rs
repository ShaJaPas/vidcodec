//! CUDA device handle shared by NVENC sessions on one GPU.

use alloc::sync::Arc;

use cudarc::driver::CudaContext;
use vidcodec_core::Error;

/// Shared CUDA context for NVENC (`NV_ENC_DEVICE_TYPE_CUDA`).
pub(crate) struct Device {
    cuda: Arc<CudaContext>,
}

impl Device {
    /// Opens GPU `index` (typically `0`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::backend`] when the NVIDIA driver or CUDA is unavailable.
    pub(crate) fn open(index: usize) -> Result<Arc<Self>, Error> {
        let cuda = CudaContext::new(index).map_err(|err| Error::backend(err.to_string()))?;
        Ok(Arc::new(Self { cuda }))
    }

    /// CUDA context passed to [`nvidia_video_codec_sdk::Encoder::initialize_with_cuda`].
    pub(crate) fn cuda(&self) -> Arc<CudaContext> {
        Arc::clone(&self.cuda)
    }
}
