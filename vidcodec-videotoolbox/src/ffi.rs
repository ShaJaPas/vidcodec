//! CoreMedia FFI helpers not exposed by `apple-cf`.

use core::ffi::c_void;
use core::time::Duration;

use apple_cf::cm::format_description::CMFormatDescription;
use apple_cf::cm::sample_buffer::CMSampleBuffer;
use apple_cf::cm::time::CMTime;
use vidcodec_bitstream::h264::{NAL_TYPE_PPS, NAL_TYPE_SPS, nal_header, split_annex_b};
use vidcodec_core::Error;

use crate::error::map_osstatus;

/// `kCMBlockBufferAlwaysCopyDataFlag` — copy payload into block-buffer-owned memory.
const BLOCK_BUFFER_ALWAYS_COPY: u32 = 0x4000_0000;
type CFAllocatorRef = *const c_void;
type CMFormatDescriptionRef = *mut c_void;
type CMBlockBufferRef = *mut c_void;
type CMSampleBufferRef = *mut c_void;
type CMItemCount = usize;

#[repr(C)]
struct CMSampleTimingInfo {
    duration: CMTime,
    presentation_time_stamp: CMTime,
    decode_time_stamp: CMTime,
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: CFAllocatorRef,
        parameter_set_count: usize,
        parameter_set_pointers: *const *const u8,
        parameter_set_sizes: *const usize,
        nal_unit_header_length: i32,
        format_description_out: *mut CMFormatDescriptionRef,
    ) -> i32;

    fn CMBlockBufferCreateWithMemoryBlock(
        structure_allocator: CFAllocatorRef,
        memory_block: *mut c_void,
        block_length: usize,
        block_allocator: CFAllocatorRef,
        custom_block_source: *const c_void,
        offset_to_data: usize,
        data_length: usize,
        flags: u32,
        block_buffer_out: *mut CMBlockBufferRef,
    ) -> i32;

    fn CMSampleBufferCreateReady(
        allocator: CFAllocatorRef,
        data_buffer: CMBlockBufferRef,
        format_description: CMFormatDescriptionRef,
        sample_count: CMItemCount,
        sample_timing_entry_count: CMItemCount,
        sample_timing_array: *const CMSampleTimingInfo,
        sample_size_entry_count: CMItemCount,
        sample_size_array: *const usize,
        sample_buffer_out: *mut CMSampleBufferRef,
    ) -> i32;
}

/// Builds an H.264 `CMFormatDescription` from Annex-B SPS/PPS extradata.
pub(crate) fn h264_format_from_extradata(extradata: &[u8]) -> Result<CMFormatDescription, Error> {
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    for nal in split_annex_b(extradata) {
        if nal.is_empty() {
            continue;
        }
        let (_, _, nal_type) = nal_header(nal[0]);
        match nal_type {
            NAL_TYPE_SPS => sps.push(nal.to_vec()),
            NAL_TYPE_PPS => pps.push(nal.to_vec()),
            _ => {}
        }
    }
    if sps.is_empty() || pps.is_empty() {
        return Err(Error::backend("missing SPS or PPS in extradata"));
    }

    let mut parameter_sets = Vec::new();
    parameter_sets.extend(sps);
    parameter_sets.extend(pps);

    let pointers: Vec<*const u8> = parameter_sets.iter().map(|n| n.as_ptr()).collect();
    let sizes: Vec<usize> = parameter_sets.iter().map(|n| n.len()).collect();
    let mut out: CMFormatDescriptionRef = core::ptr::null_mut();

    // SAFETY: pointers/sizes reference valid NAL buffers for the duration of the call.
    unsafe {
        map_osstatus(
            CMVideoFormatDescriptionCreateFromH264ParameterSets(
                core::ptr::null(),
                parameter_sets.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                &mut out,
            ),
            "CMVideoFormatDescriptionCreateFromH264ParameterSets",
        )?;
    }

    CMFormatDescription::from_raw(out).ok_or_else(|| Error::backend("null format description"))
}

/// Wraps a length-prefixed H.264 access unit in a `CMSampleBuffer`.
pub(crate) fn sample_buffer_from_avcc(
    format: &CMFormatDescription,
    payload: &[u8],
    pts: Duration,
) -> Result<CMSampleBuffer, Error> {
    let mut block: CMBlockBufferRef = core::ptr::null_mut();
    let mut storage = payload.to_vec();
    // SAFETY: CMBlockBufferCreateWithMemoryBlock adopts `storage` memory.
    unsafe {
        map_osstatus(
            CMBlockBufferCreateWithMemoryBlock(
                core::ptr::null(),
                storage.as_mut_ptr().cast(),
                storage.len(),
                core::ptr::null(),
                core::ptr::null(),
                0,
                storage.len(),
                BLOCK_BUFFER_ALWAYS_COPY,
                &mut block,
            ),
            "CMBlockBufferCreateWithMemoryBlock",
        )?;
    }

    let micros = pts.as_micros().min(i64::MAX as u128) as i64;
    let timing = CMSampleTimingInfo {
        duration: CMTime::indefinite(),
        presentation_time_stamp: CMTime::new(micros, 1_000_000),
        decode_time_stamp: CMTime::indefinite(),
    };
    let sample_size = storage.len();
    let mut sample: CMSampleBufferRef = core::ptr::null_mut();

    // SAFETY: block + format are valid CoreMedia objects.
    unsafe {
        map_osstatus(
            CMSampleBufferCreateReady(
                core::ptr::null(),
                block,
                format.as_ptr(),
                1,
                1,
                &timing,
                1,
                &sample_size,
                &mut sample,
            ),
            "CMSampleBufferCreateReady",
        )?;
    }

    CMSampleBuffer::from_raw(sample).ok_or_else(|| Error::backend("null CMSampleBuffer"))
}
