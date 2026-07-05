//! `IMFMediaType` helpers.

use vidcodec_core::{EncoderConfig, Error};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE,
    MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_SUBTYPE,
    MFCreateMediaType, MFMediaType_Video, MFVideoFormat_H264, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive,
};

use crate::error::WinResultExt;
use crate::profile::profile_to_mf;

/// Creates an H.264 output media type for the encoder MFT.
pub(crate) fn create_h264_output_type(config: &EncoderConfig) -> Result<IMFMediaType, Error> {
    // SAFETY: MFCreateMediaType initializes a new COM object.
    let media_type = unsafe { MFCreateMediaType().mf()? };

    let profile =
        profile_to_mf(config.profile).ok_or(Error::InvalidConfig("unsupported H.264 profile"))?;

    // SAFETY: Setting attributes on a freshly created IMFMediaType is safe and follows the COM contract.
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .mf()?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
            .mf()?;
        media_type
            .SetUINT32(&MF_MT_AVG_BITRATE, config.bitrate)
            .mf()?;
        media_type.SetUINT32(&MF_MT_MPEG2_PROFILE, profile).mf()?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .mf()?;
        media_type
            .SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)
            .mf()?;
        media_type
            .SetUINT64(
                &MF_MT_FRAME_SIZE,
                (config.width as u64) << 32 | config.height as u64,
            )
            .mf()?;
        media_type
            .SetUINT64(
                &MF_MT_FRAME_RATE,
                (config.frame_rate.0 as u64) << 32 | config.frame_rate.1 as u64,
            )
            .mf()?;
    }

    Ok(media_type)
}

/// Creates an NV12 input media type for the encoder MFT.
pub(crate) fn create_nv12_input_type(config: &EncoderConfig) -> Result<IMFMediaType, Error> {
    // SAFETY: MFCreateMediaType initializes a new COM object.
    let media_type = unsafe { MFCreateMediaType().mf()? };

    // SAFETY: Setting attributes on a freshly created IMFMediaType is safe and follows the COM contract.
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .mf()?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .mf()?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .mf()?;
        media_type
            .SetUINT64(
                &MF_MT_FRAME_SIZE,
                (config.width as u64) << 32 | config.height as u64,
            )
            .mf()?;
        media_type
            .SetUINT64(
                &MF_MT_FRAME_RATE,
                (config.frame_rate.0 as u64) << 32 | config.frame_rate.1 as u64,
            )
            .mf()?;
    }

    Ok(media_type)
}

/// Creates an H.264 decoder input media type.
pub(crate) fn create_h264_decoder_input_type(
    width: u32,
    height: u32,
    sequence_header: Option<&[u8]>,
) -> Result<IMFMediaType, Error> {
    use windows::Win32::Media::MediaFoundation::MF_MT_MPEG_SEQUENCE_HEADER;

    // SAFETY: MFCreateMediaType initializes a new COM object.
    let media_type = unsafe { MFCreateMediaType().mf()? };

    // SAFETY: Setting attributes on a freshly created IMFMediaType is safe and follows the COM contract.
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .mf()?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
            .mf()?;
        if width > 0 && height > 0 {
            media_type
                .SetUINT64(&MF_MT_FRAME_SIZE, (width as u64) << 32 | height as u64)
                .mf()?;
        }
        if let Some(header) = sequence_header {
            media_type
                .SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, header)
                .mf()?;
        }
    }

    Ok(media_type)
}

/// Creates an NV12 decoder output media type.
pub(crate) fn create_nv12_output_type(width: u32, height: u32) -> Result<IMFMediaType, Error> {
    // SAFETY: MFCreateMediaType initializes a new COM object.
    let media_type = unsafe { MFCreateMediaType().mf()? };

    // SAFETY: Setting attributes on a freshly created IMFMediaType is safe and follows the COM contract.
    unsafe {
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .mf()?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .mf()?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .mf()?;
        if width > 0 && height > 0 {
            media_type
                .SetUINT64(&MF_MT_FRAME_SIZE, (width as u64) << 32 | height as u64)
                .mf()?;
        }
    }

    Ok(media_type)
}

/// Reads `MF_MT_FRAME_SIZE` from a media type.
pub(crate) fn frame_size_from_type(media_type: &IMFMediaType) -> Result<(u32, u32), Error> {
    // SAFETY: GetUINT64 reads a packed size attribute; safe with a valid IMFMediaType.
    let packed = unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE).mf()? };
    Ok(((packed >> 32) as u32, packed as u32))
}
