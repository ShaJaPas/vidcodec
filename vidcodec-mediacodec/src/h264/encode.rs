//! H.264 hardware encoder via Android MediaCodec.

use core::time::Duration;

use bytes::Bytes;
use ndk::media::media_codec::{
    DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;
use ndk::media_error::MediaError;
use vidcodec_bitstream::h264::{
    annex_b_to_length_prefixed, collect_parameter_sets_annex_b, contains_idr,
};
use vidcodec_core::{
    BackendId, BitstreamFormat, CodecCapability, EncodedUnit, EncoderConfig, Error, PixelFormat,
    VideoEncoder, VideoFrame,
};

use crate::error::map_media;
use crate::profile::is_supported;
use vidcodec_util::pixel::i420_to_nv12;

const BUFFER_FLAG_KEY_FRAME: u32 = 1;
const BUFFER_FLAG_CODEC_CONFIG: u32 = 2;
const BUFFER_FLAG_END_OF_STREAM: u32 = 4;
const DEQUEUE_TIMEOUT: Duration = Duration::from_millis(10);

/// H.264 MediaCodec encoder session.
pub(crate) struct H264Encoder {
    capability: CodecCapability,
    config: EncoderConfig,
    codec: MediaCodec,
    force_idr: bool,
    frame_count: u64,
    last_pts: Option<Duration>,
    parameter_sets: Option<Bytes>,
}

// SAFETY: ndk's MediaCodec is !Send + !Sync but the encoder wrapper is used from a single thread at a time and may be moved between threads.
unsafe impl Send for H264Encoder {}

impl H264Encoder {
    /// Opens an H.264 encoder for `cap` using `config`.
    pub(crate) fn open(capability: CodecCapability, config: EncoderConfig) -> Result<Self, Error> {
        validate_encoder_config(&config, &capability)?;

        let codec = MediaCodec::from_encoder_type("video/avc").ok_or(Error::NoBackend {
            codec: config.codec,
            backend: BackendId::MediaCodec,
            direction: vidcodec_core::Direction::Encode,
        })?;

        let mut fmt = MediaFormat::new();
        build_encoder_format(&mut fmt, &config);

        codec
            .configure(&fmt, None, MediaCodecDirection::Encoder)
            .map_err(|e| map_media(e, "configure encoder"))?;

        codec.start().map_err(|e| map_media(e, "start encoder"))?;

        Ok(Self {
            capability,
            config,
            codec,
            force_idr: true,
            frame_count: 0,
            last_pts: None,
            parameter_sets: None,
        })
    }

    /// Drains all available output packets from the encoder.
    fn drain_output(&mut self) -> Result<Vec<EncodedUnit>, Error> {
        let mut units = Vec::new();
        loop {
            match self.codec.dequeue_output_buffer(DEQUEUE_TIMEOUT) {
                Ok(DequeuedOutputBufferInfoResult::Buffer(out_buf)) => {
                    let info = *out_buf.info();
                    let flags = info.flags();

                    // Codec config (SPS/PPS) — save as parameter sets.
                    if flags & BUFFER_FLAG_CODEC_CONFIG != 0 {
                        let data = out_buf.buffer().to_vec();
                        self.parameter_sets = Some(Bytes::from(data));
                        self.codec
                            .release_output_buffer(out_buf, false)
                            .map_err(|e| map_media(e, "release config buffer"))?;
                        continue;
                    }

                    let offset = info.offset() as usize;
                    let size = info.size() as usize;
                    let raw = out_buf.buffer();
                    let raw_data = raw[offset..offset + size].to_vec();
                    let pts_us = info.presentation_time_us().max(0) as u64;

                    self.codec
                        .release_output_buffer(out_buf, false)
                        .map_err(|e| map_media(e, "release output buffer"))?;

                    let annex_b = Bytes::from(raw_data);
                    let data = match self.config.bitstream {
                        BitstreamFormat::AnnexB => annex_b.clone(),
                        BitstreamFormat::LengthPrefixed => {
                            Bytes::from(annex_b_to_length_prefixed(&annex_b))
                        }
                        BitstreamFormat::Av1Obu => {
                            return Err(Error::InvalidConfig("AV1 OBU output invalid for H.264"));
                        }
                    };

                    let is_keyframe =
                        (flags & BUFFER_FLAG_KEY_FRAME) != 0 || contains_idr(&annex_b);

                    if is_keyframe {
                        let ps = collect_parameter_sets_annex_b(&annex_b);
                        if !ps.is_empty() {
                            self.parameter_sets = Some(Bytes::from(ps));
                        }
                    }

                    units.push(EncodedUnit::new(
                        data,
                        is_keyframe,
                        Duration::from_micros(pts_us),
                        self.config.bitstream,
                    ));
                }
                Ok(DequeuedOutputBufferInfoResult::TryAgainLater)
                | Ok(DequeuedOutputBufferInfoResult::OutputFormatChanged)
                | Ok(DequeuedOutputBufferInfoResult::OutputBuffersChanged) => break,
                Err(MediaError::ErrorWouldBlock) => break,
                Err(e) => return Err(map_media(e, "dequeue_output_buffer")),
            }
        }
        Ok(units)
    }
}

impl VideoEncoder for H264Encoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn reconfigure(&mut self, config: EncoderConfig) -> Result<(), Error> {
        validate_encoder_config(&config, &self.capability)?;
        self.codec.stop().ok();
        *self = Self::open(self.capability.clone(), config)?;
        Ok(())
    }

    fn set_bitrate(&mut self, bitrate_bps: u32) {
        let mut params = MediaFormat::new();
        params.set_i32("bitrate", bitrate_bps.min(i32::MAX as u32) as i32);
        let _ = self.codec.set_parameters(params);
        self.config.bitrate = bitrate_bps;
    }

    fn force_keyframe(&mut self) {
        self.force_idr = true;
    }

    fn encode(&mut self, frame: VideoFrame<'_>) -> Result<Vec<EncodedUnit>, Error> {
        frame.validate()?;
        if frame.format != self.config.input_format {
            return Err(Error::InvalidConfig(
                "frame format does not match encoder config",
            ));
        }
        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(Error::InvalidConfig("frame dimensions mismatch"));
        }

        let should_idr = self.force_idr
            || self.frame_count == 0
            || self.frame_count % u64::from(self.config.gop_size) == 0;
        self.force_idr = false;

        let nv12_buf;
        let pixels = if frame.format == PixelFormat::I420 {
            nv12_buf = i420_to_nv12(frame.pixels, frame.width, frame.height)?;
            nv12_buf.as_slice()
        } else {
            frame.pixels
        };

        let frame_size = PixelFormat::Nv12
            .frame_size(frame.width, frame.height)
            .map_err(|_| Error::InvalidConfig("frame too large"))?;

        let mut input_buf = loop {
            match self.codec.dequeue_input_buffer(DEQUEUE_TIMEOUT) {
                Ok(DequeuedInputBufferResult::Buffer(buf)) => break buf,
                Ok(DequeuedInputBufferResult::TryAgainLater) => continue,
                Err(MediaError::ErrorWouldBlock) => continue,
                Err(e) => return Err(map_media(e, "dequeue_input_buffer")),
            }
        };

        let dst = input_buf.buffer_mut();
        if dst.len() < frame_size {
            return Err(Error::backend("input buffer too small for frame"));
        }
        // SAFETY: pixels is valid for frame_size bytes, dst is writable,
        // and MaybeUninit<u8> has the same layout as u8.
        unsafe {
            core::ptr::copy_nonoverlapping(
                pixels.as_ptr(),
                dst.as_mut_ptr().cast::<u8>(),
                frame_size,
            );
        }

        let pts_us = frame.pts.as_micros().min(u64::MAX as u128) as u64;
        let flags = if should_idr { BUFFER_FLAG_KEY_FRAME } else { 0 };

        self.codec
            .queue_input_buffer(input_buf, 0, frame_size, pts_us, flags)
            .map_err(|e| map_media(e, "queue_input_buffer"))?;

        self.frame_count += 1;
        self.last_pts = Some(frame.pts);

        self.drain_output()
    }

    fn drain(&mut self) -> Result<Vec<EncodedUnit>, Error> {
        if let Ok(DequeuedInputBufferResult::Buffer(buf)) =
            self.codec.dequeue_input_buffer(DEQUEUE_TIMEOUT)
        {
            let _ = self
                .codec
                .queue_input_buffer(buf, 0, 0, 0, BUFFER_FLAG_END_OF_STREAM);
        }
        self.drain_output()
    }

    fn last_encoded_pts(&self) -> Option<Duration> {
        self.last_pts
    }

    fn parameter_sets(&self) -> Option<Bytes> {
        self.parameter_sets.clone()
    }
}

fn validate_encoder_config(
    config: &EncoderConfig,
    capability: &CodecCapability,
) -> Result<(), Error> {
    config.validate()?;
    if config.codec != vidcodec_core::CodecId::H264 {
        return Err(Error::InvalidConfig("H264Encoder requires H.264"));
    }
    if config.input_format != PixelFormat::Nv12 && config.input_format != PixelFormat::I420 {
        return Err(Error::InvalidConfig(
            "MediaCodec H.264 expects NV12 or I420 input",
        ));
    }
    if !matches!(
        config.bitstream,
        BitstreamFormat::AnnexB | BitstreamFormat::LengthPrefixed
    ) {
        return Err(Error::InvalidConfig(
            "unsupported H.264 bitstream output format",
        ));
    }
    if !capability.supports(config.profile, config.width, config.height) {
        return Err(Error::InvalidConfig("capability does not support config"));
    }
    if !is_supported(config.profile) {
        return Err(Error::InvalidConfig("unsupported H.264 profile"));
    }
    Ok(())
}

fn build_encoder_format(fmt: &mut MediaFormat, config: &EncoderConfig) {
    fmt.set_str("mime", "video/avc");
    fmt.set_i32("width", config.width as i32);
    fmt.set_i32("height", config.height as i32);
    fmt.set_i32("bitrate", config.bitrate.min(i32::MAX as u32) as i32);

    let fps = if config.frame_rate.1 > 0 {
        (config.frame_rate.0 / config.frame_rate.1) as i32
    } else {
        30
    };
    fmt.set_i32("frame-rate", fps.max(1));

    let iframe_interval = if config.gop_size > 0 {
        (config.gop_size / fps.max(1) as u32).max(1)
    } else {
        1
    };
    fmt.set_i32("i-frame-interval", iframe_interval as i32);

    // COLOR_FormatYUV420SemiPlanar = NV12
    fmt.set_i32("color-format", 21);

    if let Ok(frame_size) = PixelFormat::Nv12.frame_size(config.width, config.height) {
        fmt.set_i32("max-input-size", frame_size.min(i32::MAX as usize) as i32);
    }
}
