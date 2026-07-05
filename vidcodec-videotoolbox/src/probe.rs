//! Capability probing via VideoToolbox encoder/decoder availability.

use vidcodec_core::{BackendId, BitstreamFormat, CodecCapability, CodecId, Direction, Error};
use videotoolbox::compression::CompressionSession;
use videotoolbox::decompression::DecompressionSession;
use videotoolbox::session::Codec;

use vidcodec_util::profile::default_h264_profiles;

/// Conservative upper bound when per-codec caps are not queried.
const DEFAULT_MAX_DIMENSION: u32 = 8192;

/// Probes H.264 encode/decode support.
pub(crate) fn probe() -> Result<Vec<CodecCapability>, Error> {
    let mut caps = Vec::new();

    if probe_encoder_present() {
        let mut builder =
            CodecCapability::builder(CodecId::H264, BackendId::VideoToolbox, Direction::Encode)
                .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
                .bitstream_formats(vec![
                    BitstreamFormat::AnnexB,
                    BitstreamFormat::LengthPrefixed,
                ])
                .low_latency(true);
        for profile in default_h264_profiles() {
            builder = builder.profile(*profile);
        }
        caps.push(builder.build());
    }

    if DecompressionSession::is_hardware_decode_supported(Codec::H264) {
        let mut builder =
            CodecCapability::builder(CodecId::H264, BackendId::VideoToolbox, Direction::Decode)
                .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
                .bitstream_formats(vec![
                    BitstreamFormat::AnnexB,
                    BitstreamFormat::LengthPrefixed,
                ])
                .low_latency(true);
        for profile in default_h264_profiles() {
            builder = builder.profile(*profile);
        }
        caps.push(builder.build());
    }

    Ok(caps)
}

fn probe_encoder_present() -> bool {
    CompressionSession::builder(320, 240, Codec::H264)
        .with_real_time(true)
        .build()
        .is_ok()
}
