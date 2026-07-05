//! Video encoder configuration and trait.

use core::time::Duration;

use alloc::vec::Vec;
use bytes::Bytes;

use crate::{BitstreamFormat, CodecId, EncodedUnit, Error, PixelFormat, Profile, VideoFrame};

/// Parameters for opening or reconfiguring a hardware encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderConfig {
    /// Target video codec.
    pub codec: CodecId,
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Frame rate as `(numerator, denominator)` — e.g. `(30, 1)` for 30 fps.
    pub frame_rate: (u32, u32),
    /// Target bitrate in bits per second.
    pub bitrate: u32,
    /// Optional peak bitrate cap (VBR).
    pub max_bitrate: Option<u32>,
    /// Negotiated profile.
    pub profile: Profile,
    /// Expected uncompressed input layout.
    pub input_format: PixelFormat,
    /// How encoded units are emitted.
    pub bitstream: BitstreamFormat,
    /// Prefer low-latency presets (no B-frames, short GOP).
    pub low_latency: bool,
    /// Maximum interval between keyframes in frames (`1` = all-intra).
    pub gop_size: u32,
    /// Maximum encoder buffering before [`Error::Backpressure`] is returned.
    pub max_inflight_frames: u32,
}

impl EncoderConfig {
    /// Creates a config with real-time defaults for `width × height` at `frame_rate`.
    #[must_use]
    pub fn new(width: u32, height: u32, frame_rate: (u32, u32)) -> Self {
        Self {
            codec: CodecId::H264,
            width,
            height,
            frame_rate,
            bitrate: 2_000_000,
            max_bitrate: None,
            profile: Profile::H264Main,
            input_format: PixelFormat::Nv12,
            bitstream: BitstreamFormat::AnnexB,
            low_latency: true,
            gop_size: 60,
            max_inflight_frames: 2,
        }
    }

    /// Sets the codec family and a matching default profile.
    #[must_use]
    pub const fn with_codec(mut self, codec: CodecId) -> Self {
        self.codec = codec;
        self.profile = default_profile(codec);
        self.bitstream = default_bitstream(codec);
        self
    }

    /// Sets target bitrate (bits per second).
    #[must_use]
    pub const fn with_bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Sets optional peak bitrate for VBR encoders.
    #[must_use]
    pub const fn with_max_bitrate(mut self, max_bitrate: u32) -> Self {
        self.max_bitrate = Some(max_bitrate);
        self
    }

    /// Sets negotiated profile (must match [`Self::codec`]).
    #[must_use]
    pub const fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
        self
    }

    /// Sets expected input pixel format.
    #[must_use]
    pub const fn with_input_format(mut self, format: PixelFormat) -> Self {
        self.input_format = format;
        self
    }

    /// Sets output bitstream framing.
    #[must_use]
    pub const fn with_bitstream(mut self, bitstream: BitstreamFormat) -> Self {
        self.bitstream = bitstream;
        self
    }

    /// Validates fields.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] when values are inconsistent.
    pub fn validate(&self) -> Result<(), Error> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::InvalidConfig("width and height must be non-zero"));
        }
        if self.frame_rate.0 == 0 || self.frame_rate.1 == 0 {
            return Err(Error::InvalidConfig("frame_rate must be non-zero"));
        }
        if self.bitrate == 0 {
            return Err(Error::InvalidConfig("bitrate must be non-zero"));
        }
        if self.profile.codec() != self.codec {
            return Err(Error::InvalidConfig("profile does not match codec"));
        }
        if !BitstreamFormat::for_codec(self.codec).contains(&self.bitstream) {
            return Err(Error::InvalidConfig("bitstream format invalid for codec"));
        }
        if self.gop_size == 0 {
            return Err(Error::InvalidConfig("gop_size must be non-zero"));
        }
        Ok(())
    }
}

const fn default_profile(codec: CodecId) -> Profile {
    match codec {
        CodecId::H264 => Profile::H264Main,
        CodecId::Hevc => Profile::HevcMain,
        CodecId::Av1 => Profile::Av1Main,
    }
}

const fn default_bitstream(codec: CodecId) -> BitstreamFormat {
    match codec {
        CodecId::H264 | CodecId::Hevc => BitstreamFormat::AnnexB,
        CodecId::Av1 => BitstreamFormat::Av1Obu,
    }
}

/// Hardware video encoder instance.
pub trait VideoEncoder: Send {
    /// Active capability descriptor for this instance.
    fn capability(&self) -> &crate::CodecCapability;

    /// Applies a new configuration without tearing down the session when possible.
    ///
    /// # Errors
    ///
    /// Propagates backend failures and [`Error::InvalidConfig`].
    fn reconfigure(&mut self, config: EncoderConfig) -> Result<(), Error>;

    /// Updates target bitrate without a full reconfigure (for congestion control).
    fn set_bitrate(&mut self, bitrate_bps: u32);

    /// Requests the next output access unit to be a keyframe / sync sample.
    fn force_keyframe(&mut self);

    /// Encodes one uncompressed frame.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Backpressure`] when the internal queue is full.
    fn encode(&mut self, frame: VideoFrame<'_>) -> Result<Vec<EncodedUnit>, Error>;

    /// Drains any delayed access units (end-of-stream or flush).
    ///
    /// # Errors
    ///
    /// Propagates backend failures.
    fn drain(&mut self) -> Result<Vec<EncodedUnit>, Error> {
        Ok(Vec::new())
    }

    /// Returns the timestamp of the last successfully encoded frame, if any.
    fn last_encoded_pts(&self) -> Option<Duration> {
        None
    }

    /// Returns cached SPS/PPS (Annex-B) after the most recent IDR, when available.
    fn parameter_sets(&self) -> Option<Bytes> {
        None
    }
}
