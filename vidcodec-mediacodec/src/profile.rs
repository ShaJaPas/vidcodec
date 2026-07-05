//! Profile mapping between vidcodec and MediaCodec.

use vidcodec_core::Profile;
use vidcodec_util::profile::default_h264_profiles;

/// MediaCodec doesn't surface profile negotiation — it accepts any stream the
/// hardware supports.  We validate profiles against what the device typically
/// handles.
pub(crate) fn is_supported(profile: Profile) -> bool {
    default_h264_profiles().contains(&profile) || profile == Profile::H264ConstrainedBaseline
}
