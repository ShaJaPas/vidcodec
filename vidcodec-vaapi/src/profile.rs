//! Mapping between libva profiles/entrypoints and vidcodec types.

use vaapi_sys::{
    VAEntrypoint, VAEntrypoint_VAEntrypointEncSlice, VAEntrypoint_VAEntrypointEncSliceLP,
    VAEntrypoint_VAEntrypointVLD, VAProfile, VAProfile_VAProfileH264Baseline,
    VAProfile_VAProfileH264ConstrainedBaseline, VAProfile_VAProfileH264High,
    VAProfile_VAProfileH264Main,
};
use vidcodec_core::{CodecId, Direction, Profile};

/// Returns the vidcodec profile for a VA H.264 profile constant, if known.
#[must_use]
#[allow(non_upper_case_globals)]
pub(crate) fn va_profile_to_vidcodec(profile: VAProfile) -> Option<Profile> {
    match profile {
        VAProfile_VAProfileH264Baseline => Some(Profile::H264Baseline),
        VAProfile_VAProfileH264ConstrainedBaseline => Some(Profile::H264ConstrainedBaseline),
        VAProfile_VAProfileH264Main => Some(Profile::H264Main),
        VAProfile_VAProfileH264High => Some(Profile::H264High),
        _ => None,
    }
}

/// Returns the VA profile for a vidcodec H.264 profile.
#[must_use]
pub(crate) fn vidcodec_profile_to_va(profile: Profile) -> Option<VAProfile> {
    match profile {
        Profile::H264Baseline => Some(VAProfile_VAProfileH264Baseline),
        Profile::H264ConstrainedBaseline => Some(VAProfile_VAProfileH264ConstrainedBaseline),
        Profile::H264Main => Some(VAProfile_VAProfileH264Main),
        Profile::H264High => Some(VAProfile_VAProfileH264High),
        _ => None,
    }
}

/// Maps H.264 SPS `profile_idc` to a VA profile for decode context creation.
#[must_use]
pub(crate) fn sps_to_va_profile(sps: &vidcodec_bitstream::h264::H264Sps) -> VAProfile {
    match sps.profile_idc {
        66 => VAProfile_VAProfileH264ConstrainedBaseline,
        77 => VAProfile_VAProfileH264Main,
        88 => VAProfile_VAProfileH264Main,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135 => {
            VAProfile_VAProfileH264High
        }
        _ => VAProfile_VAProfileH264Main,
    }
}

/// Returns encode/decode direction for a VA entrypoint, if supported.
#[must_use]
#[allow(non_upper_case_globals)]
pub(crate) fn entrypoint_direction(entrypoint: VAEntrypoint) -> Option<Direction> {
    match entrypoint {
        VAEntrypoint_VAEntrypointEncSlice | VAEntrypoint_VAEntrypointEncSliceLP => {
            Some(Direction::Encode)
        }
        VAEntrypoint_VAEntrypointVLD => Some(Direction::Decode),
        _ => None,
    }
}

/// Preferred VA encode entrypoint (low-latency slice LP when available).
#[must_use]
pub(crate) fn pick_encode_entrypoint(entrypoints: &[VAEntrypoint]) -> Option<VAEntrypoint> {
    if entrypoints.contains(&VAEntrypoint_VAEntrypointEncSliceLP) {
        Some(VAEntrypoint_VAEntrypointEncSliceLP)
    } else if entrypoints.contains(&VAEntrypoint_VAEntrypointEncSlice) {
        Some(VAEntrypoint_VAEntrypointEncSlice)
    } else {
        None
    }
}

/// Returns H.264 [`CodecId`] when `profile` is an H.264 VA profile.
#[must_use]
pub(crate) fn va_profile_codec(profile: VAProfile) -> Option<CodecId> {
    va_profile_to_vidcodec(profile).map(|_| CodecId::H264)
}

/// Picks an H.264 level_idc for VA sequence parameters.
#[must_use]
pub(crate) fn h264_level_idc(width: u32, height: u32) -> u8 {
    let pixels = width.saturating_mul(height);
    if pixels <= 1280 * 720 {
        31 // Level 3.1
    } else if pixels <= 1920 * 1080 {
        40 // Level 4.0
    } else if pixels <= 3840 * 2160 {
        51 // Level 5.1
    } else {
        52 // Level 5.2
    }
}
