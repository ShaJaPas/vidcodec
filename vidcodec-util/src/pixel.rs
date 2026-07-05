//! Pixel format conversion helpers shared across backends.

use vidcodec_core::{Error, PixelFormat};

/// Converts packed I420 into tightly-packed NV12.
///
/// I420: Y plane, U plane, V plane (each plane contiguous).
/// NV12: Y plane, interleaved UV plane (U at even offsets, V at odd).
///
/// # Errors
///
/// Returns [`Error::PixelBufferMismatch`] when `i420` length does not match
/// `PixelFormat::I420.frame_size(width, height)`.
pub fn i420_to_nv12(i420: &[u8], width: u32, height: u32) -> Result<Vec<u8>, Error> {
    let expected = PixelFormat::I420.frame_size(width, height)?;
    if i420.len() != expected {
        return Err(Error::PixelBufferMismatch {
            expected,
            actual: i420.len(),
        });
    }

    let w = width as usize;
    let h = height as usize;
    let y_size = w * h;
    let uv_w = w / 2;
    let uv_h = h / 2;
    let u_src = y_size;
    let v_src = u_src + uv_w * uv_h;

    let mut nv12 = vec![0u8; y_size + y_size / 2];
    nv12[..y_size].copy_from_slice(&i420[..y_size]);

    let uv_off = y_size;
    for row in 0..uv_h {
        for col in 0..uv_w {
            let u = i420[u_src + row * uv_w + col];
            let v = i420[v_src + row * uv_w + col];
            let dst = uv_off + row * w + col * 2;
            nv12[dst] = u;
            nv12[dst + 1] = v;
        }
    }

    Ok(nv12)
}

/// Copies NV12 pixels from a strided (padded) source into a tightly-packed
/// destination.
///
/// `width` and `height` are the visible frame dimensions.  `stride` is the Y
/// plane row stride in bytes (≥ width).  `slice_height` is the Y plane
/// allocation height (≥ height).
pub fn copy_nv12_tight(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    stride: usize,
    slice_height: usize,
) {
    let dst_y_stride = width;
    let src_y_stride = stride;

    for y in 0..height {
        let src_off = y * src_y_stride;
        let dst_off = y * dst_y_stride;
        dst[dst_off..dst_off + width].copy_from_slice(&src[src_off..src_off + width]);
    }

    let src_uv_base = stride * slice_height;
    let dst_uv_base = width * height;
    let uv_height = height / 2;
    for y in 0..uv_height {
        let src_off = src_uv_base + y * src_y_stride;
        let dst_off = dst_uv_base + y * dst_y_stride;
        dst[dst_off..dst_off + width].copy_from_slice(&src[src_off..src_off + width]);
    }
}
