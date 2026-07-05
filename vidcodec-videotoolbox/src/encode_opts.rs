//! VideoToolbox per-frame encode options.

use core::ffi::c_void;

use apple_cf::cf::{AsCFType, CFDictionary, CFString, CFType};
use apple_cf::cm::{CMSampleBuffer, CMTime};
use apple_cf::raw::kCFBooleanTrue;
use vidcodec_core::Error;
use videotoolbox::compression::CompressionSession;

use crate::error::map_vt;

type CFStringRef = *const c_void;

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    static kVTEncodeFrameOptionKey_ForceKeyFrame: CFStringRef;
}

/// Builds `kVTEncodeFrameOptionKey_ForceKeyFrame = true`.
pub(crate) fn force_keyframe_properties() -> CFDictionary {
    // SAFETY: `kVTEncodeFrameOptionKey_ForceKeyFrame` is a process-lifetime static.
    let key = unsafe {
        CFString::from_raw(kVTEncodeFrameOptionKey_ForceKeyFrame.cast_mut().cast())
            .expect("kVTEncodeFrameOptionKey_ForceKeyFrame")
    };
    // SAFETY: `kCFBooleanTrue` is a process-lifetime static.
    let value =
        unsafe { CFType::from_raw(kCFBooleanTrue.cast_mut().cast()).expect("kCFBooleanTrue") };
    CFDictionary::from_pairs(&[(&key as &dyn AsCFType, &value as &dyn AsCFType)])
}

/// Encodes one frame with `ForceKeyFrame` via the async VT API (blocked synchronously).
pub(crate) fn encode_forced_keyframe(
    session: &CompressionSession,
    pixel_buffer: apple_cf::cv::CVPixelBuffer,
    presentation_time: (i64, i32),
) -> Result<Vec<u8>, Error> {
    let pts = CMTime::new(presentation_time.0, presentation_time.1);
    let sample = pollster::block_on(session.encode_frame_async(
        pixel_buffer,
        pts,
        CMTime::indefinite(),
        Some(force_keyframe_properties()),
    ))
    .map_err(map_vt)?;
    sample_buffer_data(&sample)
}

fn sample_buffer_data(sample: &CMSampleBuffer) -> Result<Vec<u8>, Error> {
    let block = sample
        .data_buffer()
        .ok_or_else(|| Error::backend("encoded sample has no data buffer"))?;
    block
        .copy_data_bytes(0, block.data_length())
        .ok_or_else(|| Error::backend("CMBlockBufferCopyDataBytes failed"))
}
