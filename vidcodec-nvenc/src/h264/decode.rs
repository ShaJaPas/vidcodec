//! H.264 NVDEC decoder.

use alloc::sync::Arc;

use vidcodec_bitstream::h264::length_prefixed_to_annex_b;
use vidcodec_core::{
    BitstreamFormat, CodecCapability, DecodedFrame, DecoderConfig, EncodedUnit, Error, VideoDecoder,
};

use crate::device::Device;
use crate::nvdec::VideoParser;

/// H.264 NVDEC decoder.
pub(crate) struct H264Decoder {
    capability: CodecCapability,
    config: DecoderConfig,
    parser: VideoParser,
    extradata_loaded: bool,
}

// SAFETY: NVDEC sessions are tied to one CUDA context.
unsafe impl Send for H264Decoder {}

impl H264Decoder {
    /// Opens an H.264 decoder for `cap`.
    pub(crate) fn open(
        device: Arc<Device>,
        capability: CodecCapability,
        config: DecoderConfig,
    ) -> Result<Self, Error> {
        config.validate()?;
        if config.codec != vidcodec_core::CodecId::H264 {
            return Err(Error::InvalidConfig("H264Decoder requires H.264"));
        }
        if config.output_format != vidcodec_core::PixelFormat::Nv12 {
            return Err(Error::InvalidConfig("NVDEC H.264 outputs NV12"));
        }
        if !BitstreamFormat::for_codec(config.codec).contains(&config.bitstream) {
            return Err(Error::InvalidConfig("unsupported H.264 bitstream format"));
        }

        let parser = VideoParser::create_h264(device.cuda(), config.output_format)?;

        Ok(Self {
            capability,
            config,
            parser,
            extradata_loaded: false,
        })
    }

    fn bootstrap_extradata(&mut self) -> Result<(), Error> {
        if let Some(extradata) = self.config.extradata.clone() {
            let _ = self.parser.feed(&extradata, core::time::Duration::ZERO)?;
        }
        Ok(())
    }

    fn unit_to_annex_b(&self, unit: &EncodedUnit) -> Result<Vec<u8>, Error> {
        match unit.bitstream {
            BitstreamFormat::AnnexB => Ok(unit.data.to_vec()),
            BitstreamFormat::LengthPrefixed => {
                length_prefixed_to_annex_b(&unit.data).map_err(bitstream_err)
            }
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
            self.bootstrap_extradata()?;
            self.extradata_loaded = true;
        }
        if unit.data.is_empty() {
            return Err(Error::InvalidBitstream("empty access unit"));
        }

        let annex_b = self.unit_to_annex_b(unit)?;
        let frames = self.parser.feed(&annex_b, unit.pts)?;
        if frames.is_empty() {
            return Err(Error::InvalidBitstream("decoder produced no frames"));
        }
        Ok(frames)
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.parser.reset_decoder()?;
        self.extradata_loaded = false;
        Ok(())
    }
}

fn bitstream_err(err: vidcodec_bitstream::BitstreamError) -> Error {
    Error::backend(err.to_string())
}
