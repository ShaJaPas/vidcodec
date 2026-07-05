//! H.264 inverse quantization matrix for VA-API decode.

use core::mem;

use vaapi_sys::VAIQMatrixBufferH264;

/// Builds flat scaling lists (value 16) for drivers that require an explicit IQ matrix.
#[must_use]
pub(crate) fn default_iq_matrix() -> VAIQMatrixBufferH264 {
    // SAFETY: `VAIQMatrixBufferH264` is plain-old-data; zeroed memory is a valid initial state before filling scaling lists.
    let mut iq = unsafe { mem::zeroed::<VAIQMatrixBufferH264>() };
    for list in &mut iq.ScalingList4x4 {
        list.fill(16);
    }
    for list in &mut iq.ScalingList8x8 {
        list.fill(16);
    }
    iq
}
