//! Video decoder configuration and trait.

use alloc::vec::Vec;
use bytes::Bytes;

use crate::{BitstreamFormat, CodecId, DecodedFrame, EncodedUnit, Error, PixelFormat};

/// Parameters for opening a hardware decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderConfig {
    /// Expected video codec.
    pub codec: CodecId,
    /// How incoming [`EncodedUnit`] payloads are framed.
    pub bitstream: BitstreamFormat,
    /// Preferred output pixel layout.
    pub output_format: PixelFormat,
    /// Low-latency decode tuning when supported by the backend.
    pub low_latency: bool,
    /// Optional out-of-band SPS/PPS (Annex-B) supplied at session start.
    pub extradata: Option<Bytes>,
}

impl DecoderConfig {
    /// Creates a decoder config with real-time defaults for `codec`.
    #[must_use]
    pub const fn new(codec: CodecId) -> Self {
        Self {
            codec,
            bitstream: default_bitstream(codec),
            output_format: PixelFormat::Nv12,
            low_latency: true,
            extradata: None,
        }
    }

    /// Sets expected bitstream framing.
    #[must_use]
    pub const fn with_bitstream(mut self, bitstream: BitstreamFormat) -> Self {
        self.bitstream = bitstream;
        self
    }

    /// Sets preferred decoded pixel format.
    #[must_use]
    pub const fn with_output_format(mut self, format: PixelFormat) -> Self {
        self.output_format = format;
        self
    }

    /// Sets out-of-band codec parameter sets (Annex-B SPS/PPS for H.264).
    #[must_use]
    pub fn with_extradata(mut self, extradata: impl Into<Bytes>) -> Self {
        self.extradata = Some(extradata.into());
        self
    }

    /// Validates fields.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] when values are inconsistent.
    pub fn validate(&self) -> Result<(), Error> {
        if !BitstreamFormat::for_codec(self.codec).contains(&self.bitstream) {
            return Err(Error::InvalidConfig("bitstream format invalid for codec"));
        }
        Ok(())
    }
}

const fn default_bitstream(codec: CodecId) -> BitstreamFormat {
    match codec {
        CodecId::H264 | CodecId::Hevc => BitstreamFormat::AnnexB,
        CodecId::Av1 => BitstreamFormat::Av1Obu,
    }
}

/// Hardware video decoder instance.
pub trait VideoDecoder: Send {
    /// Active capability descriptor for this instance.
    fn capability(&self) -> &crate::CodecCapability;

    /// Feeds one compressed access unit.
    ///
    /// May return zero frames when more input is required (codec delay).
    ///
    /// # Errors
    ///
    /// Propagates backend failures and [`Error::InvalidBitstream`].
    fn decode(&mut self, unit: &EncodedUnit) -> Result<Vec<DecodedFrame>, Error>;

    /// Flushes internal delay buffers at end-of-stream.
    ///
    /// # Errors
    ///
    /// Propagates backend failures.
    fn flush(&mut self) -> Result<Vec<DecodedFrame>, Error> {
        Ok(Vec::new())
    }

    /// Resets decoder state after a keyframe request or large gap.
    ///
    /// # Errors
    ///
    /// Propagates backend failures.
    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
