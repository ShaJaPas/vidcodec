//! Device → host NV12 copy helpers and host-side format conversion.

use alloc::sync::Arc;

use cudarc::driver::{CudaContext, sys::cuMemcpyDtoH_v2};
use vidcodec_core::{Error, PixelFormat};

/// Copies a pitched NV12 device frame into a tight host buffer.
pub(crate) fn copy_nv12_from_device(
    cuda: &Arc<CudaContext>,
    dev_ptr: u64,
    pitch: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Error> {
    cuda.bind_to_thread()
        .map_err(|err| Error::backend(err.to_string()))?;

    let expected = PixelFormat::Nv12.frame_size(width, height)?;
    let mut pixels = vec![0u8; expected];
    let pitch = pitch as usize;
    let width = width as usize;
    let height = height as usize;

    // SAFETY: CUDA context is bound; `dev_ptr` is a mapped NVDEC frame.
    unsafe {
        for row in 0..height {
            let dst = pixels.as_mut_ptr().add(row * width);
            let src = dev_ptr + (row * pitch) as u64;
            map_cuda(cuMemcpyDtoH_v2(dst.cast(), src, width), "cuMemcpyDtoH Y")?;
        }

        let uv_dev = dev_ptr + (pitch * height) as u64;
        let uv_off = width * height;
        for row in 0..height / 2 {
            let dst = pixels.as_mut_ptr().add(uv_off + row * width);
            let src = uv_dev + (row * pitch) as u64;
            map_cuda(cuMemcpyDtoH_v2(dst.cast(), src, width), "cuMemcpyDtoH UV")?;
        }
    }

    Ok(pixels)
}

fn map_cuda(result: cudarc::driver::sys::CUresult, context: &str) -> Result<(), Error> {
    crate::error::map_cuda(result, context)
}
