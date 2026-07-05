//! vidcodec profile ↔ MF attribute mapping.

use vidcodec_core::Profile;
use windows::Win32::Media::MediaFoundation::{
    eAVEncH264VProfile_Base, eAVEncH264VProfile_High, eAVEncH264VProfile_Main,
};

/// Maps a vidcodec H.264 profile to `MF_MT_MPEG2_PROFILE`.
pub(crate) fn profile_to_mf(profile: Profile) -> Option<u32> {
    match profile {
        Profile::H264Baseline => Some(eAVEncH264VProfile_Base.0 as u32),
        Profile::H264Main => Some(eAVEncH264VProfile_Main.0 as u32),
        Profile::H264High => Some(eAVEncH264VProfile_High.0 as u32),
        _ => None,
    }
}
