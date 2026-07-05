//! Maps NVENC GUIDs to [`vidcodec`] types.

use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    GUID, NV_ENC_CODEC_AV1_GUID, NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID,
    NV_ENC_H264_PROFILE_BASELINE_GUID, NV_ENC_H264_PROFILE_CONSTRAINED_HIGH_GUID,
    NV_ENC_H264_PROFILE_HIGH_GUID, NV_ENC_H264_PROFILE_MAIN_GUID, NV_ENC_HEVC_PROFILE_MAIN_GUID,
    NV_ENC_HEVC_PROFILE_MAIN10_GUID,
};
use vidcodec_core::{CodecId, Profile};

/// Returns the NVENC codec GUID for `codec`.
pub(crate) fn codec_to_guid(codec: CodecId) -> GUID {
    match codec {
        CodecId::H264 => NV_ENC_CODEC_H264_GUID,
        CodecId::Hevc => NV_ENC_CODEC_HEVC_GUID,
        CodecId::Av1 => NV_ENC_CODEC_AV1_GUID,
    }
}

/// Maps an H.264 NVENC profile GUID to [`Profile`].
pub(crate) fn h264_guid_to_profile(guid: GUID) -> Option<Profile> {
    if guid == NV_ENC_H264_PROFILE_BASELINE_GUID {
        Some(Profile::H264Baseline)
    } else if guid == NV_ENC_H264_PROFILE_MAIN_GUID {
        Some(Profile::H264Main)
    } else if guid == NV_ENC_H264_PROFILE_HIGH_GUID
        || guid == NV_ENC_H264_PROFILE_CONSTRAINED_HIGH_GUID
    {
        Some(Profile::H264High)
    } else {
        None
    }
}

/// Maps [`Profile`] to an H.264 NVENC profile GUID.
pub(crate) fn profile_to_h264_guid(profile: Profile) -> Option<GUID> {
    Some(match profile {
        Profile::H264Baseline | Profile::H264ConstrainedBaseline => {
            NV_ENC_H264_PROFILE_BASELINE_GUID
        }
        Profile::H264Main => NV_ENC_H264_PROFILE_MAIN_GUID,
        Profile::H264High => NV_ENC_H264_PROFILE_HIGH_GUID,
        _ => return None,
    })
}

/// Maps an HEVC NVENC profile GUID to [`Profile`].
pub(crate) fn hevc_guid_to_profile(guid: GUID) -> Option<Profile> {
    if guid == NV_ENC_HEVC_PROFILE_MAIN_GUID {
        Some(Profile::HevcMain)
    } else if guid == NV_ENC_HEVC_PROFILE_MAIN10_GUID {
        Some(Profile::HevcMain10)
    } else {
        None
    }
}
