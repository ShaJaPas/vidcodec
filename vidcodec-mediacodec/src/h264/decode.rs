//! H.264 hardware decoder via Android MediaCodec.

use core::time::Duration;

use bytes::Bytes;
use ndk::media::media_codec::{
    DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;
use ndk::media_error::MediaError;
use vidcodec_bitstream::h264::{
    NAL_TYPE_PPS, NAL_TYPE_SPS, length_prefixed_to_annex_b, nal_header, split_annex_b,
};
use vidcodec_core::{
    BackendId, BitstreamFormat, CodecCapability, DecodedFrame, DecoderConfig, EncodedUnit, Error,
    PixelFormat, VideoDecoder,
};

use crate::error::map_media;
use vidcodec_util::pixel::copy_nv12_tight;

const BUFFER_FLAG_END_OF_STREAM: u32 = 4;
const DEQUEUE_TIMEOUT: Duration = Duration::from_millis(10);

/// H.264 MediaCodec decoder session.
pub(crate) struct H264Decoder {
    capability: CodecCapability,
    config: DecoderConfig,
    codec: MediaCodec,
    width: u32,
    height: u32,
    stride: u32,
    slice_height: u32,
    extradata_loaded: bool,
}

// SAFETY: ndk's MediaCodec is !Send + !Sync but the decoder wrapper is used from a single thread at a time and may be moved between threads.
unsafe impl Send for H264Decoder {}

impl H264Decoder {
    /// Opens an H.264 decoder for `cap`.
    pub(crate) fn open(capability: CodecCapability, config: DecoderConfig) -> Result<Self, Error> {
        config.validate()?;
        if config.codec != vidcodec_core::CodecId::H264 {
            return Err(Error::InvalidConfig("H264Decoder requires H.264"));
        }
        if config.output_format != PixelFormat::Nv12 {
            return Err(Error::InvalidConfig(
                "MediaCodec H.264 decoder outputs NV12",
            ));
        }
        if !BitstreamFormat::for_codec(config.codec).contains(&config.bitstream) {
            return Err(Error::InvalidConfig("unsupported H.264 bitstream format"));
        }

        let codec = MediaCodec::from_decoder_type("video/avc").ok_or(Error::NoBackend {
            codec: config.codec,
            backend: BackendId::MediaCodec,
            direction: vidcodec_core::Direction::Decode,
        })?;

        let mut fmt = MediaFormat::new();
        let extradata_loaded = build_decoder_format(&mut fmt, &config);

        codec
            .configure(&fmt, None, MediaCodecDirection::Decoder)
            .map_err(|e| map_media(e, "configure decoder"))?;

        codec.start().map_err(|e| map_media(e, "start decoder"))?;

        Ok(Self {
            capability,
            config,
            codec,
            width: 0,
            height: 0,
            stride: 0,
            slice_height: 0,
            extradata_loaded,
        })
    }

    /// Reads current output format and updates cached dimensions.
    fn update_output_dimensions(&mut self) {
        let fmt = self.codec.output_format();

        if let Some(w) = fmt.i32("width") {
            self.width = w.max(0) as u32;
        }
        if let Some(h) = fmt.i32("height") {
            self.height = h.max(0) as u32;
        }
        self.stride = fmt
            .i32("stride")
            .map(|s| s.max(0) as u32)
            .unwrap_or(self.width);
        self.slice_height = fmt
            .i32("slice-height")
            .map(|s| s.max(0) as u32)
            .unwrap_or(self.height);
    }

    /// Drains all available decoded frames from the output port.
    fn dequeue_output_frames(&mut self) -> Result<Vec<DecodedFrame>, Error> {
        let mut frames = Vec::new();

        loop {
            match self.codec.dequeue_output_buffer(DEQUEUE_TIMEOUT) {
                Ok(DequeuedOutputBufferInfoResult::Buffer(out_buf)) => {
                    let info = *out_buf.info();
                    let flags = info.flags();

                    if flags & BUFFER_FLAG_END_OF_STREAM != 0 {
                        self.codec
                            .release_output_buffer(out_buf, false)
                            .map_err(|e| map_media(e, "release EOS buffer"))?;
                        break;
                    }

                    if info.size() <= 0 {
                        self.codec
                            .release_output_buffer(out_buf, false)
                            .map_err(|e| map_media(e, "release empty buffer"))?;
                        continue;
                    }

                    let offset = info.offset() as usize;
                    let size = info.size() as usize;
                    let raw_data = out_buf.buffer()[offset..offset + size].to_vec();
                    let pts_us = info.presentation_time_us().max(0) as u64;

                    self.codec
                        .release_output_buffer(out_buf, false)
                        .map_err(|e| map_media(e, "release output buffer"))?;

                    let w = self.width.max(1) as usize;
                    let h = self.height.max(1) as usize;
                    let stride = self.stride.max(1) as usize;
                    let slice_h = self.slice_height.max(1) as usize;

                    let frame_size = PixelFormat::Nv12
                        .frame_size(w as u32, h as u32)
                        .unwrap_or(w * h * 3 / 2);

                    let mut tight_pixels = vec![0u8; frame_size];
                    copy_nv12_tight(&raw_data, &mut tight_pixels, w, h, stride, slice_h);

                    frames.push(DecodedFrame {
                        pixels: Bytes::from(tight_pixels),
                        width: w as u32,
                        height: h as u32,
                        format: self.config.output_format,
                        pts: Duration::from_micros(pts_us),
                    });
                }
                Ok(DequeuedOutputBufferInfoResult::OutputFormatChanged)
                | Ok(DequeuedOutputBufferInfoResult::OutputBuffersChanged) => {
                    let old_w = self.width;
                    let old_h = self.height;
                    self.update_output_dimensions();
                    if self.width != old_w || self.height != old_h {
                        continue;
                    }
                    break;
                }
                Ok(DequeuedOutputBufferInfoResult::TryAgainLater) => break,
                Err(MediaError::ErrorWouldBlock) => break,
                Err(e) => return Err(map_media(e, "dequeue_output_buffer")),
            }
        }

        Ok(frames)
    }

    /// Converts an encoded unit payload to Annex B format.
    fn unit_to_annex_b(&self, unit: &EncodedUnit) -> Result<Vec<u8>, Error> {
        match unit.bitstream {
            BitstreamFormat::AnnexB => Ok(unit.data.to_vec()),
            BitstreamFormat::LengthPrefixed => {
                length_prefixed_to_annex_b(&unit.data).map_err(|e| Error::backend(e.to_string()))
            }
            BitstreamFormat::Av1Obu => {
                Err(Error::InvalidBitstream("AV1 bitstream in H.264 decoder"))
            }
        }
    }

    /// Feeds an access unit into the decoder.
    fn feed_to_decoder(&mut self, data: &[u8], pts_us: u64) -> Result<(), Error> {
        let mut input_buf = loop {
            match self.codec.dequeue_input_buffer(DEQUEUE_TIMEOUT) {
                Ok(DequeuedInputBufferResult::Buffer(buf)) => break buf,
                Ok(DequeuedInputBufferResult::TryAgainLater) => continue,
                Err(MediaError::ErrorWouldBlock) => continue,
                Err(e) => return Err(map_media(e, "dequeue_input_buffer")),
            }
        };

        let dst = input_buf.buffer_mut();
        if dst.len() < data.len() {
            return Err(Error::backend("input buffer too small for access unit"));
        }
        // SAFETY: data is valid for data.len() bytes, dst is writable,
        // and MaybeUninit<u8> has the same layout as u8.
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                dst.as_mut_ptr().cast::<u8>(),
                data.len(),
            );
        }

        self.codec
            .queue_input_buffer(input_buf, 0, data.len(), pts_us, 0)
            .map_err(|e| map_media(e, "queue_input_buffer"))?;

        Ok(())
    }
}

impl VideoDecoder for H264Decoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn decode(&mut self, unit: &EncodedUnit) -> Result<Vec<DecodedFrame>, Error> {
        if unit.data.is_empty() {
            return Err(Error::InvalidBitstream("empty access unit"));
        }

        if !self.extradata_loaded {
            return Err(Error::InvalidBitstream(
                "decoder needs extradata before first decode; \
                 pass SPS/PPS via DecoderConfig::with_extradata",
            ));
        }

        let annex_b = self.unit_to_annex_b(unit)?;
        let pts_us = unit.pts.as_micros().min(u64::MAX as u128) as u64;

        self.feed_to_decoder(&annex_b, pts_us)?;
        self.dequeue_output_frames()
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>, Error> {
        self.codec
            .flush()
            .map_err(|e| map_media(e, "flush decoder"))?;
        self.dequeue_output_frames()
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.codec
            .flush()
            .map_err(|e| map_media(e, "reset decoder"))?;
        self.width = 0;
        self.height = 0;
        self.stride = 0;
        self.slice_height = 0;
        Ok(())
    }
}

/// Sets `csd-0` (SPS) and `csd-1` (PPS) on the decoder format from
/// Annex-B extradata.  Returns `true` when at least `csd-0` was set.
fn build_decoder_format(fmt: &mut MediaFormat, config: &DecoderConfig) -> bool {
    fmt.set_str("mime", "video/avc");

    let Some(ref extradata) = config.extradata else {
        return false;
    };

    let nals = split_annex_b(extradata);
    if nals.is_empty() {
        return false;
    }

    let mut has_csd0 = false;
    for nal in nals {
        if nal.is_empty() {
            continue;
        }
        let (_, _, nal_type) = nal_header(nal[0]);
        match nal_type {
            NAL_TYPE_SPS => {
                fmt.set_buffer("csd-0", nal);
                has_csd0 = true;
            }
            NAL_TYPE_PPS => {
                fmt.set_buffer("csd-1", nal);
            }
            _ => {}
        }
    }

    has_csd0
}
