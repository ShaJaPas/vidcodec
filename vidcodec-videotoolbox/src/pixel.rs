//! Host ↔ `CVPixelBuffer` NV12 helpers.

use apple_cf::cv::{CVPixelBuffer, CVPixelBufferLockFlags};
use vidcodec_core::{Error, PixelFormat};

/// NV12 bi-planar video-range (`kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`).
pub(crate) const NV12_VIDEO_RANGE: u32 = u32::from_be_bytes(*b"420v");

/// Copies packed NV12 host pixels into a newly created `CVPixelBuffer`.
pub(crate) fn copy_nv12_to_pixel_buffer(
    buffer: &CVPixelBuffer,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let expected = PixelFormat::Nv12.frame_size(width, height)?;
    if pixels.len() != expected {
        return Err(Error::PixelBufferMismatch {
            expected,
            actual: pixels.len(),
        });
    }

    let w = width as usize;
    let h = height as usize;
    let mut guard = buffer
        .lock(CVPixelBufferLockFlags::NONE)
        .map_err(|status| Error::backend(format!("CVPixelBuffer lock: {status}")))?;

    if buffer.plane_count() != 2 {
        return Err(Error::backend("expected bi-planar NV12 pixel buffer"));
    }

    let y_stride = buffer.bytes_per_row_of_plane(0);
    let uv_stride = buffer.bytes_per_row_of_plane(1);
    let y_plane = guard
        .base_address_of_plane_mut(0)
        .ok_or_else(|| Error::backend("missing Y plane base address"))?;
    let uv_plane = guard
        .base_address_of_plane_mut(1)
        .ok_or_else(|| Error::backend("missing UV plane base address"))?;

    // SAFETY: planes are locked for write; sizes match NV12 layout.
    unsafe {
        for row in 0..h {
            let src = pixels.as_ptr().add(row * w);
            let dst = y_plane.add(row * y_stride);
            core::ptr::copy_nonoverlapping(src, dst, w);
        }
        let uv_off = w * h;
        for row in 0..h / 2 {
            let src = pixels.as_ptr().add(uv_off + row * w);
            let dst = uv_plane.add(row * uv_stride);
            core::ptr::copy_nonoverlapping(src, dst, w);
        }
    }

    Ok(())
}

/// Reads a bi-planar NV12 `CVPixelBuffer` into a tight host buffer.
pub(crate) fn read_nv12_from_pixel_buffer(
    buffer: &CVPixelBuffer,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Error> {
    let expected = PixelFormat::Nv12.frame_size(width, height)?;
    let mut pixels = vec![0u8; expected];
    let w = width as usize;
    let h = height as usize;

    let guard = buffer
        .lock(CVPixelBufferLockFlags::READ_ONLY)
        .map_err(|status| Error::backend(format!("CVPixelBuffer lock: {status}")))?;

    let y_stride = buffer.bytes_per_row_of_plane(0);
    let uv_stride = buffer.bytes_per_row_of_plane(1);
    let y_plane = guard
        .base_address_of_plane(0)
        .ok_or_else(|| Error::backend("missing Y plane base address"))?;
    let uv_plane = guard
        .base_address_of_plane(1)
        .ok_or_else(|| Error::backend("missing UV plane base address"))?;

    // SAFETY: planes are locked for read.
    unsafe {
        for row in 0..h {
            let src = y_plane.add(row * y_stride);
            let dst = pixels.as_mut_ptr().add(row * w);
            core::ptr::copy_nonoverlapping(src, dst, w);
        }
        let uv_off = w * h;
        for row in 0..h / 2 {
            let src = uv_plane.add(row * uv_stride);
            let dst = pixels.as_mut_ptr().add(uv_off + row * w);
            core::ptr::copy_nonoverlapping(src, dst, w);
        }
    }

    Ok(pixels)
}

/// Creates an NV12 `CVPixelBuffer` and fills it with `pixels`.
pub(crate) fn nv12_pixel_buffer_from_host(
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<CVPixelBuffer, Error> {
    let buffer = CVPixelBuffer::create(width as usize, height as usize, NV12_VIDEO_RANGE)
        .map_err(|status| Error::backend(format!("CVPixelBuffer::create: {status}")))?;
    copy_nv12_to_pixel_buffer(&buffer, pixels, width, height)?;
    Ok(buffer)
}
