//! Integration tests for the public registry API.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use serial_test::serial;
use vidcodec::{
    Backend, BackendId, CodecCapability, CodecId, DecoderConfig, Direction, EncodedUnit,
    EncoderConfig, Error, PixelFormat, Profile, VideoDecoder, VideoEncoder, VideoFrame, enumerate,
    enumerate_codec, open_encoder, register, reset_registry,
};

struct MockBackend {
    id: BackendId,
}

impl Backend for MockBackend {
    fn id(&self) -> BackendId {
        self.id
    }

    fn enumerate(&self, direction: Direction) -> Vec<CodecCapability> {
        vec![
            CodecCapability::builder(CodecId::H264, self.id, direction)
                .profile(Profile::H264Main)
                .build(),
        ]
    }

    fn open_encoder(
        &self,
        cap: &CodecCapability,
        config: EncoderConfig,
    ) -> Result<Box<dyn VideoEncoder>, Error> {
        Ok(Box::new(MockEncoder {
            cap: cap.clone(),
            config,
        }))
    }

    fn open_decoder(
        &self,
        cap: &CodecCapability,
        _config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error> {
        let _ = cap;
        Err(Error::NotImplemented("mock decoder"))
    }
}

struct MockEncoder {
    cap: CodecCapability,
    config: EncoderConfig,
}

impl VideoEncoder for MockEncoder {
    fn capability(&self) -> &CodecCapability {
        &self.cap
    }

    fn reconfigure(&mut self, config: EncoderConfig) -> Result<(), Error> {
        config.validate()?;
        self.config = config;
        Ok(())
    }

    fn set_bitrate(&mut self, bitrate_bps: u32) {
        self.config.bitrate = bitrate_bps;
    }

    fn force_keyframe(&mut self) {}

    fn encode(&mut self, frame: VideoFrame<'_>) -> Result<Vec<EncodedUnit>, Error> {
        frame.validate()?;
        Ok(vec![EncodedUnit::new(
            Bytes::from_static(b"\x00\x00\x00\x01mock"),
            true,
            frame.pts,
            self.config.bitstream,
        )])
    }
}

fn setup_vaapi() {
    reset_registry();
    register(Arc::new(MockBackend {
        id: BackendId::Vaapi,
    }));
}

#[test]
#[serial]
fn enumerate_returns_registered_capabilities() {
    setup_vaapi();
    let caps = enumerate(Direction::Encode);
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].codec, CodecId::H264);
    assert_eq!(caps[0].backend, BackendId::Vaapi);
}

#[test]
#[serial]
fn enumerate_sorts_by_backend_preference() {
    reset_registry();
    register(Arc::new(MockBackend {
        id: BackendId::Vaapi,
    }));
    register(Arc::new(MockBackend {
        id: BackendId::Nvenc,
    }));

    let caps = enumerate_codec(CodecId::H264, Direction::Encode);
    assert_eq!(caps.len(), 2);
    assert_eq!(caps[0].backend, BackendId::Nvenc);
    assert_eq!(caps[1].backend, BackendId::Vaapi);
}

#[test]
#[serial]
fn open_encoder_roundtrip() {
    setup_vaapi();
    let cap = enumerate_codec(CodecId::H264, Direction::Encode)
        .into_iter()
        .next()
        .unwrap();
    let config = EncoderConfig::new(640, 480, (30, 1)).with_codec(CodecId::H264);
    let mut encoder = open_encoder(&cap, config).unwrap();
    encoder.set_bitrate(1_000_000);
    encoder.force_keyframe();

    let frame = VideoFrame {
        pixels: &vec![0u8; PixelFormat::Nv12.frame_size(640, 480).unwrap()],
        width: 640,
        height: 480,
        format: PixelFormat::Nv12,
        pts: Duration::ZERO,
    };
    let units = encoder.encode(frame).unwrap();
    assert_eq!(units.len(), 1);
    assert!(units[0].is_keyframe);
}
