# Vidcodec

Cross-platform facade over **hardware video encoders and decoders** for real-time
applications (VoIP, screen share, low-latency streaming).

Vidcodec does **not** implement codecs and does **not** expose CPU software encoders.

It defines a small API for:

- **Probing** which hardware codec/backend combinations the host exposes.
- **Opening** an encoder or decoder for a chosen capability.
- **Streaming** raw frames in and encoded access units out (and the reverse for decode).

[`enumerate`] returns capabilities sorted by [`BackendId`] declaration order.

## Quick start

```rust,no_run
use std::time::Duration;

use vidcodec::{
    CodecId, DecoderConfig, Direction, EncoderConfig, PixelFormat, Profile, VideoFrame,
    enumerate_codec, open_decoder, open_encoder,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pick the first H.264 encoder and decoder on this host.
    let enc_cap = enumerate_codec(CodecId::H264, Direction::Encode)
        .into_iter().next().ok_or("no H.264 encoder")?;
    let dec_cap = enumerate_codec(CodecId::H264, Direction::Decode)
        .into_iter().next().ok_or("no H.264 decoder")?;

    // Open encoder (320×240, NV12, 1.5 Mbps, H.264 Main).
    let enc_config = EncoderConfig::new(320, 240, (30, 1))
        .with_codec(CodecId::H264)
        .with_profile(Profile::H264Main)
        .with_input_format(PixelFormat::Nv12)
        .with_bitrate(1_500_000);
    let mut encoder = open_encoder(&enc_cap, enc_config)?;

    // Build a dummy NV12 frame and encode it.
    let frame_size = PixelFormat::Nv12.frame_size(320, 240)?;
    let frame = VideoFrame {
        pixels: &vec![128u8; frame_size],
        width: 320,
        height: 240,
        format: PixelFormat::Nv12,
        pts: Duration::ZERO,
    };
    let units = encoder.encode(frame)?;
    let extradata = encoder.parameter_sets().expect("SPS/PPS after IDR");

    // Open decoder with the parameter sets from the encoder.
    let dec_config = DecoderConfig::new(CodecId::H264)
        .with_output_format(PixelFormat::Nv12)
        .with_extradata(extradata);
    let mut decoder = open_decoder(&dec_cap, dec_config)?;

    // Decode the first encoded unit back to NV12.
    let decoded = decoder.decode(&units[0])?;
    assert_eq!(decoded[0].width, 320);
    assert_eq!(decoded[0].height, 240);
    assert_eq!(decoded[0].format, PixelFormat::Nv12);

    println!("Encode → decode round-trip OK ({} bytes pixels)", decoded[0].pixels.len());
    Ok(())
}
```
## License

Apache-2.0
