//! H.264 NVENC encode → NVDEC decode round-trip on local NVIDIA GPU.

use core::time::Duration;

use bytes::Bytes;
use serial_test::serial;
use vidcodec_bitstream::h264::strip_parameter_sets_annex_b;
use vidcodec_core::{
    BitstreamFormat, CodecId, DecoderConfig, Direction, EncoderConfig, PixelFormat, Profile,
    VideoFrame, open_decoder, open_encoder,
};

fn test_nv12_pattern(width: u32, height: u32) -> Vec<u8> {
    let y_size = (width * height) as usize;
    let uv_size = y_size / 2;
    let mut pixels = vec![0u8; y_size + uv_size];
    for y in 0..height {
        for x in 0..width {
            pixels[y as usize * width as usize + x as usize] = ((x + y) % 256) as u8;
        }
    }
    let uv_off = y_size;
    let uv_w = width as usize;
    for row in 0..(height as usize / 2) {
        for col in 0..uv_w {
            pixels[uv_off + row * uv_w + col] = 128u8.saturating_add(((col + row) % 64) as u8);
        }
    }
    pixels
}

fn pick_cap(direction: Direction) -> vidcodec_core::CodecCapability {
    vidcodec_core::enumerate(direction)
        .into_iter()
        .find(|c| c.codec == CodecId::H264)
        .expect("H.264 capability required for round-trip test")
}

/// Minimum acceptable luma PSNR for lossy HW encode at 1.5 Mbps / 320×240.
const MIN_LUMA_PSNR_DB: f64 = 28.0;

fn luma_psnr(expected: &[u8], actual: &[u8], width: u32, height: u32) -> f64 {
    let n = (width * height) as usize;
    assert!(expected.len() >= n && actual.len() >= n);
    let mse: f64 = expected[..n]
        .iter()
        .zip(actual[..n].iter())
        .map(|(&a, &b)| {
            let d = f64::from(a) - f64::from(b);
            d * d
        })
        .sum::<f64>()
        / n as f64;
    if mse == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

fn assert_pixels_close(expected: &[u8], decoded: &[u8], width: u32, height: u32, label: &str) {
    let psnr = luma_psnr(expected, decoded, width, height);
    assert!(
        psnr >= MIN_LUMA_PSNR_DB,
        "{label}: luma PSNR {psnr:.1} dB below threshold {MIN_LUMA_PSNR_DB:.1} dB"
    );
}

#[test]
#[serial]
fn h264_annex_b_roundtrip() {
    run_roundtrip(BitstreamFormat::AnnexB);
}

#[test]
#[serial]
fn h264_length_prefixed_roundtrip() {
    run_roundtrip(BitstreamFormat::LengthPrefixed);
}

#[test]
#[serial]
fn h264_extradata_decode() {
    vidcodec_core::reset_registry();
    vidcodec_nvenc::try_register().expect("NVENC/NVDEC should be available on this host");

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 240;

    let enc_cap = pick_cap(Direction::Encode);
    let dec_cap = pick_cap(Direction::Decode);

    let enc_config = EncoderConfig::new(WIDTH, HEIGHT, (30, 1))
        .with_profile(Profile::H264Main)
        .with_input_format(PixelFormat::Nv12)
        .with_bitrate(1_500_000)
        .with_codec(CodecId::H264);

    let mut encoder = open_encoder(&enc_cap, enc_config).expect("open encoder");
    let pixels = test_nv12_pattern(WIDTH, HEIGHT);
    let frame = VideoFrame {
        pixels: &pixels,
        width: WIDTH,
        height: HEIGHT,
        format: PixelFormat::Nv12,
        pts: Duration::ZERO,
    };

    let units = encoder.encode(frame).expect("encode IDR");
    let extradata = encoder
        .parameter_sets()
        .expect("encoder should expose parameter sets after IDR");
    assert!(
        !extradata.is_empty(),
        "parameter sets should contain SPS+PPS"
    );

    let dec_config = DecoderConfig::new(CodecId::H264)
        .with_output_format(PixelFormat::Nv12)
        .with_extradata(extradata);

    let mut decoder = open_decoder(&dec_cap, dec_config).expect("open decoder with extradata");

    let decoded = decoder.decode(&units[0]).expect("decode IDR");
    assert_eq!(decoded.len(), 1);
    assert_pixels_close(
        &pixels,
        &decoded[0].pixels,
        WIDTH,
        HEIGHT,
        "IDR with extradata",
    );

    let frame2 = VideoFrame {
        pixels: &pixels,
        width: WIDTH,
        height: HEIGHT,
        format: PixelFormat::Nv12,
        pts: Duration::from_millis(33),
    };
    let units2 = encoder.encode(frame2).expect("encode P-frame");
    let annex_b = match units2[0].bitstream {
        BitstreamFormat::AnnexB => units2[0].data.to_vec(),
        BitstreamFormat::LengthPrefixed => {
            vidcodec_bitstream::h264::length_prefixed_to_annex_b(&units2[0].data).unwrap()
        }
        BitstreamFormat::Av1Obu => panic!("unexpected AV1"),
    };
    let slice_only = strip_parameter_sets_annex_b(&annex_b);
    assert!(
        !slice_only.is_empty(),
        "P-frame AU should contain slice NALs"
    );
    let p_unit = vidcodec_core::EncodedUnit::new(
        Bytes::from(slice_only),
        false,
        frame2.pts,
        BitstreamFormat::AnnexB,
    );
    let decoded2 = decoder
        .decode(&p_unit)
        .expect("decode P with extradata only");
    assert_eq!(decoded2.len(), 1);
    assert_pixels_close(
        &pixels,
        &decoded2[0].pixels,
        WIDTH,
        HEIGHT,
        "P-frame extradata",
    );

    vidcodec_core::reset_registry();
}

fn run_roundtrip(bitstream: BitstreamFormat) {
    vidcodec_core::reset_registry();
    vidcodec_nvenc::try_register().expect("NVENC/NVDEC should be available on this host");

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 240;
    const FRAMES: u32 = 8;

    let enc_cap = pick_cap(Direction::Encode);
    let dec_cap = pick_cap(Direction::Decode);

    let enc_config = EncoderConfig::new(WIDTH, HEIGHT, (30, 1))
        .with_profile(Profile::H264Main)
        .with_input_format(PixelFormat::Nv12)
        .with_bitstream(bitstream)
        .with_bitrate(1_500_000)
        .with_codec(CodecId::H264);

    let dec_config = DecoderConfig::new(CodecId::H264)
        .with_bitstream(bitstream)
        .with_output_format(PixelFormat::Nv12);

    let mut encoder = open_encoder(&enc_cap, enc_config).expect("open encoder");
    let mut decoder = open_decoder(&dec_cap, dec_config).expect("open decoder");

    let pixels = test_nv12_pattern(WIDTH, HEIGHT);

    for i in 0..FRAMES {
        let frame = VideoFrame {
            pixels: &pixels,
            width: WIDTH,
            height: HEIGHT,
            format: PixelFormat::Nv12,
            pts: Duration::from_millis(u64::from(i) * 33),
        };
        let units = encoder.encode(frame).expect("encode frame");
        assert!(!units.is_empty(), "encoder produced no access units");
        if i == 0 {
            assert!(units[0].is_keyframe, "first frame should be a keyframe");
            let collected = encoder.parameter_sets().expect("parameter sets after IDR");
            assert!(!collected.is_empty());
        }

        let decoded = decoder.decode(&units[0]).expect("decode frame");
        assert_eq!(decoded.len(), 1, "expected one decoded frame per AU");
        assert_eq!(decoded[0].width, WIDTH);
        assert_eq!(decoded[0].height, HEIGHT);
        assert_eq!(decoded[0].format, PixelFormat::Nv12);
        assert_eq!(decoded[0].pixels.len(), pixels.len());
        assert_pixels_close(&pixels, &decoded[0].pixels, WIDTH, HEIGHT, "GOP frame");
    }

    encoder.force_keyframe();
    let frame = VideoFrame {
        pixels: &pixels,
        width: WIDTH,
        height: HEIGHT,
        format: PixelFormat::Nv12,
        pts: Duration::from_millis(300),
    };
    let units = encoder.encode(frame).expect("encode forced IDR");
    assert!(units[0].is_keyframe);
    let decoded = decoder.decode(&units[0]).expect("decode forced IDR");
    assert_pixels_close(&pixels, &decoded[0].pixels, WIDTH, HEIGHT, "forced IDR");

    vidcodec_core::reset_registry();
}
