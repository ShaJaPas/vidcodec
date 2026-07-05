//! vidcodec profile ↔ VideoToolbox profile level mapping.

use vidcodec_core::Profile;
use videotoolbox::compression::ProfileLevel;

/// Maps a vidcodec H.264 profile to VideoToolbox [`ProfileLevel`].
pub(crate) fn profile_to_vt(profile: Profile) -> Option<ProfileLevel> {
    match profile {
        Profile::H264Baseline | Profile::H264ConstrainedBaseline => {
            Some(ProfileLevel::H264BaselineAutoLevel)
        }
        Profile::H264Main => Some(ProfileLevel::H264MainAutoLevel),
        Profile::H264High => Some(ProfileLevel::H264HighAutoLevel),
        _ => None,
    }
}
