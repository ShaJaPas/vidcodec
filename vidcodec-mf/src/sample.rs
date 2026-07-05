//! `IMFSample` / `IMFMediaBuffer` helpers.

use core::time::Duration;

use vidcodec_core::Error;
use windows::Win32::Media::MediaFoundation::{
    IMFMediaBuffer, IMFMediaType, IMFSample, MFCreateMemoryBuffer, MFCreateSample,
};

use crate::error::WinResultExt;

/// Creates a sample wrapping a contiguous host buffer.
pub(crate) fn create_sample_from_bytes(data: &[u8], pts: Duration) -> Result<IMFSample, Error> {
    let buffer = create_memory_buffer(data.len())?;
    copy_to_buffer(&buffer, data)?;

    // SAFETY: MFCreateSample initializes a new COM object.
    let sample = unsafe { MFCreateSample().mf()? };

    // SAFETY: AddBuffer, SetSampleTime, and SetSampleDuration are safe on a single-threaded sample.
    unsafe {
        sample.AddBuffer(&buffer).mf()?;
        let timestamp = pts.as_micros().min(i64::MAX as u128) as i64;
        sample.SetSampleTime(timestamp).mf()?;
        sample.SetSampleDuration(33_333).mf()?;
    }

    Ok(sample)
}

/// Reads a contiguous sample payload into a `Vec<u8>`.
pub(crate) fn read_sample_bytes(sample: &IMFSample) -> Result<Vec<u8>, Error> {
    // SAFETY: IMFSample::GetBufferByIndex returns a valid COM interface; safe with a valid sample.
    unsafe {
        let buffer = sample.GetBufferByIndex(0).mf()?;
        read_buffer_bytes(&buffer)
    }
}

/// Reads a locked media buffer into a `Vec<u8>`.
pub(crate) fn read_buffer_bytes(buffer: &IMFMediaBuffer) -> Result<Vec<u8>, Error> {
    // SAFETY: IMFMediaBuffer::Lock writes to caller-provided pointers; the resulting slice is valid for the locked duration.
    unsafe {
        let mut data_ptr = core::ptr::null_mut();
        let mut max_len = 0u32;
        let mut current_len = 0u32;
        buffer
            .Lock(&mut data_ptr, Some(&mut max_len), Some(&mut current_len))
            .mf()?;
        let bytes = core::slice::from_raw_parts(data_ptr.cast(), current_len as usize).to_vec();
        buffer.Unlock().mf()?;
        Ok(bytes)
    }
}

/// Copies host bytes into an existing memory buffer.
pub(crate) fn copy_to_buffer(buffer: &IMFMediaBuffer, data: &[u8]) -> Result<(), Error> {
    // SAFETY: IMFMediaBuffer::Lock, copy_nonoverlapping, and SetCurrentLength are safe with adequate capacity and valid pointers.
    unsafe {
        let mut data_ptr = core::ptr::null_mut();
        let mut max_len = 0u32;
        let mut current_len = 0u32;
        buffer
            .Lock(&mut data_ptr, Some(&mut max_len), Some(&mut current_len))
            .mf()?;
        if max_len < data.len() as u32 {
            buffer.Unlock().mf()?;
            return Err(Error::backend("MF buffer too small"));
        }
        core::ptr::copy_nonoverlapping(data.as_ptr(), data_ptr.cast(), data.len());
        buffer.SetCurrentLength(data.len() as u32).mf()?;
        buffer.Unlock().mf()?;
    }
    Ok(())
}

/// Creates an empty memory buffer with at least `capacity` bytes.
pub(crate) fn create_memory_buffer(capacity: usize) -> Result<IMFMediaBuffer, Error> {
    let capacity = u32::try_from(capacity).map_err(|_| Error::backend("buffer too large"))?;
    // SAFETY: MFCreateMemoryBuffer initializes a new COM object.
    unsafe { MFCreateMemoryBuffer(capacity).mf() }
}

/// Creates an NV12 sample.
pub(crate) fn create_nv12_sample(pixels: &[u8], pts: Duration) -> Result<IMFSample, Error> {
    create_sample_from_bytes(pixels, pts)
}

/// Reads NV12 bytes and `(width, height)` from a decoded sample.
pub(crate) fn read_nv12_sample(
    sample: &IMFSample,
    media_type: &IMFMediaType,
) -> Result<(Vec<u8>, u32, u32), Error> {
    let (width, height) = crate::media_type::frame_size_from_type(media_type)?;
    let bytes = read_sample_bytes(sample)?;
    Ok((bytes, width, height))
}
