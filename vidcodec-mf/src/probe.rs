//! Capability probing via MFT availability.

use vidcodec_core::{BackendId, BitstreamFormat, CodecCapability, CodecId, Direction, Error};

use crate::com::ensure_initialized;
use crate::mft::{create_h264_decoder, create_h264_encoder};
use vidcodec_util::profile::default_h264_profiles;

/// Conservative upper bound when per-codec caps are not queried.
const DEFAULT_MAX_DIMENSION: u32 = 4096;

/// Probes H.264 encode/decode MFT availability.
pub(crate) fn probe() -> Result<Vec<CodecCapability>, Error> {
    ensure_initialized()?;
    let mut caps = Vec::new();

    if create_h264_encoder().is_ok() {
        let mut builder =
            CodecCapability::builder(CodecId::H264, BackendId::MediaFoundation, Direction::Encode)
                .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
                .bitstream_formats(vec![
                    BitstreamFormat::AnnexB,
                    BitstreamFormat::LengthPrefixed,
                ])
                .low_latency(true);
        for &profile in default_h264_profiles() {
            builder = builder.profile(profile);
        }
        caps.push(builder.build());
    }

    if create_h264_decoder().is_ok() {
        let mut builder =
            CodecCapability::builder(CodecId::H264, BackendId::MediaFoundation, Direction::Decode)
                .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
                .bitstream_formats(vec![
                    BitstreamFormat::AnnexB,
                    BitstreamFormat::LengthPrefixed,
                ])
                .low_latency(true);
        for &profile in default_h264_profiles() {
            builder = builder.profile(profile);
        }
        caps.push(builder.build());
    }

    Ok(caps)
}
