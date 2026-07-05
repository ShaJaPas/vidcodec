//! Default H.264 profiles shared across backends.

use vidcodec_core::Profile;

/// Returns the default set of H.264 profiles advertised by most hardware
/// backends when per-codec capability probing is not available.
pub fn default_h264_profiles() -> &'static [Profile] {
    &[Profile::H264Baseline, Profile::H264Main, Profile::H264High]
}
