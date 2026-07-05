//! Capability probing via Android MediaCodec.

use ndk::media::media_codec::MediaCodec;
use vidcodec_core::{BackendId, BitstreamFormat, CodecCapability, CodecId, Direction, Error};

use vidcodec_util::profile::default_h264_profiles;

/// Conservative upper bound when per-codec caps are not queried.
const DEFAULT_MAX_DIMENSION: u32 = 4096;

/// Probes H.264 encode/decode support on this Android device.
pub(crate) fn probe() -> Result<Vec<CodecCapability>, Error> {
    let mut caps = Vec::new();

    if probe_codec_available("video/avc", true) {
        let mut builder =
            CodecCapability::builder(CodecId::H264, BackendId::MediaCodec, Direction::Encode)
                .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
                .bitstream_formats(vec![
                    BitstreamFormat::AnnexB,
                    BitstreamFormat::LengthPrefixed,
                ])
                .low_latency(true);
        for prof in default_h264_profiles() {
            builder = builder.profile(*prof);
        }
        caps.push(builder.build());
    }

    if probe_codec_available("video/avc", false) {
        let mut builder =
            CodecCapability::builder(CodecId::H264, BackendId::MediaCodec, Direction::Decode)
                .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
                .bitstream_formats(vec![
                    BitstreamFormat::AnnexB,
                    BitstreamFormat::LengthPrefixed,
                ])
                .low_latency(true);
        for prof in default_h264_profiles() {
            builder = builder.profile(*prof);
        }
        caps.push(builder.build());
    }

    Ok(caps)
}

/// Attempts to create (and immediately destroy) a MediaCodec instance for the
/// given MIME type.  Returns `true` when the device exposes a matching codec.
fn probe_codec_available(mime: &str, is_encoder: bool) -> bool {
    let codec = if is_encoder {
        MediaCodec::from_encoder_type(mime)
    } else {
        MediaCodec::from_decoder_type(mime)
    };
    codec.is_some()
}
