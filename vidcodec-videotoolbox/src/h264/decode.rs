//! H.264 hardware decoder via VideoToolbox.

use alloc::sync::Arc;
use std::sync::Mutex;

use apple_cf::cm::format_description::CMFormatDescription;
use apple_cf::cv::CVPixelBuffer;
use bytes::Bytes;
use vidcodec_bitstream::h264::{
    annex_b_to_length_prefixed, collect_parameter_sets_annex_b, length_prefixed_to_annex_b,
};
use vidcodec_core::{
    BitstreamFormat, CodecCapability, DecodedFrame, DecoderConfig, EncodedUnit, Error, PixelFormat,
    VideoDecoder,
};
use videotoolbox::decompression::{DecodedFrame as VtDecodedFrame, DecompressionSession};

use crate::error::map_vt;
use crate::ffi::{h264_format_from_extradata, sample_buffer_from_avcc};
use crate::pixel::read_nv12_from_pixel_buffer;

/// H.264 VideoToolbox decoder.
pub(crate) struct H264Decoder {
    capability: CodecCapability,
    config: DecoderConfig,
    format: Option<CMFormatDescription>,
    session: Option<DecompressionSession>,
    frames: Arc<Mutex<Vec<DecodedFrame>>>,
    width: u32,
    height: u32,
    extradata_loaded: bool,
}

// SAFETY: H264Decoder owns only Send types (Arc<Mutex<...>>, CMFormatDescription, DecompressionSession).
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
                "VideoToolbox H.264 decoder outputs NV12",
            ));
        }
        if !BitstreamFormat::for_codec(config.codec).contains(&config.bitstream) {
            return Err(Error::InvalidConfig("unsupported H.264 bitstream format"));
        }

        let format = config
            .extradata
            .as_ref()
            .map(|ps| h264_format_from_extradata(ps))
            .transpose()?;

        Ok(Self {
            capability,
            config,
            format: format.clone(),
            session: None,
            frames: Arc::new(Mutex::new(Vec::new())),
            width: 0,
            height: 0,
            extradata_loaded: format.is_some(),
        })
    }

    fn ensure_session(&mut self, annex_b: &[u8]) -> Result<(), Error> {
        if self.session.is_some() {
            return Ok(());
        }

        if self.format.is_none() {
            let ps = collect_parameter_sets_annex_b(annex_b);
            if ps.is_empty() {
                return Err(Error::InvalidBitstream(
                    "decoder needs SPS/PPS in extradata or keyframe access unit",
                ));
            }
            self.format = Some(h264_format_from_extradata(&ps)?);
        }

        let format = self
            .format
            .as_ref()
            .ok_or_else(|| Error::backend("missing H.264 format description"))?;

        let frames = Arc::clone(&self.frames);
        let output_format = self.config.output_format;

        let session = DecompressionSession::new(format, move |frame: VtDecodedFrame| {
            let out = frame
                .image_buffer
                .and_then(|buffer| decode_vt_frame(buffer, output_format, frame.presentation_time));
            if let Some(out) = out
                && let Ok(mut guard) = frames.lock()
            {
                guard.push(out);
            }
        })
        .map_err(map_vt)?;

        session.set_real_time(true).map_err(map_vt)?;
        self.session = Some(session);
        Ok(())
    }

    fn unit_payload(&self, unit: &EncodedUnit) -> Result<Vec<u8>, Error> {
        match unit.bitstream {
            BitstreamFormat::AnnexB => Ok(annex_b_to_length_prefixed(&unit.data)),
            BitstreamFormat::LengthPrefixed => Ok(unit.data.to_vec()),
            BitstreamFormat::Av1Obu => {
                Err(Error::InvalidBitstream("AV1 bitstream in H.264 decoder"))
            }
        }
    }
}

impl VideoDecoder for H264Decoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn decode(&mut self, unit: &EncodedUnit) -> Result<Vec<DecodedFrame>, Error> {
        if !self.extradata_loaded {
            if let Some(extradata) = self.config.extradata.clone() {
                self.format = Some(h264_format_from_extradata(&extradata)?);
            }
            self.extradata_loaded = true;
        }
        if unit.data.is_empty() {
            return Err(Error::InvalidBitstream("empty access unit"));
        }

        let annex_b = match unit.bitstream {
            BitstreamFormat::AnnexB => unit.data.to_vec(),
            BitstreamFormat::LengthPrefixed => length_prefixed_to_annex_b(&unit.data)
                .map_err(|err| Error::backend(err.to_string()))?,
            BitstreamFormat::Av1Obu => {
                return Err(Error::InvalidBitstream("AV1 bitstream in H.264 decoder"));
            }
        };

        self.ensure_session(&annex_b)?;
        let format = self
            .format
            .as_ref()
            .ok_or_else(|| Error::backend("missing H.264 format description"))?;
        let payload = self.unit_payload(unit)?;
        let sample = sample_buffer_from_avcc(format, &payload, unit.pts)?;

        self.frames.lock().expect("frame mutex").clear();
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| Error::backend("decoder session not initialized"))?;
        session.decode(&sample).map_err(map_vt)?;
        session.wait_for_async_frames().map_err(map_vt)?;

        let frames: Vec<DecodedFrame> =
            self.frames.lock().expect("frame mutex").drain(..).collect();
        if frames.is_empty() {
            return Err(Error::InvalidBitstream("decoder produced no frames"));
        }

        if self.width == 0
            && let Ok(buffer) = session.copy_black_pixel_buffer()
        {
            self.width = buffer.width() as u32;
            self.height = buffer.height() as u32;
        }

        Ok(frames)
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.session = None;
        if self.config.extradata.is_none() {
            self.format = None;
        }
        self.extradata_loaded = self.format.is_some();
        self.width = 0;
        self.height = 0;
        Ok(())
    }
}

fn decode_vt_frame(
    buffer: CVPixelBuffer,
    format: PixelFormat,
    presentation_time: (i64, i32),
) -> Option<DecodedFrame> {
    let width = buffer.width() as u32;
    let height = buffer.height() as u32;
    let pixels = read_nv12_from_pixel_buffer(&buffer, width, height).ok()?;
    let micros = if presentation_time.1 > 0 {
        (presentation_time.0 as i128 * 1_000_000 / presentation_time.1 as i128) as u64
    } else {
        0
    };
    Some(DecodedFrame {
        pixels: Bytes::from(pixels),
        width,
        height,
        format,
        pts: core::time::Duration::from_micros(micros),
    })
}
