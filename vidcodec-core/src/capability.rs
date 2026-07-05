//! Platform backend and capability descriptors.

use alloc::vec::Vec;

use crate::{BitstreamFormat, CodecId, Direction, Profile};

/// Platform-specific video engine that implements a codec.
///
/// Variants are declared **most-preferred first**. [`enumerate`](crate::probe::enumerate) sorts capabilities
/// by this order (then by [`CodecId`]), so the first matching entry is the best
/// local path for that codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BackendId {
    /// Apple VideoToolbox (macOS, iOS) — fixed-function block on Apple Silicon / T2.
    VideoToolbox,
    /// NVIDIA NVENC / NVDEC — dedicated video engine on GeForce / Quadro / RTX.
    Nvenc,
    /// Linux VA-API (Intel/AMD iGPU, Mesa, `nvidia-vaapi-driver` for decode).
    Vaapi,
    /// Android NDK MediaCodec — SoC video block.
    MediaCodec,
    /// Windows Media Foundation — routes to GPU vendor codecs via D3D.
    MediaFoundation,
}

impl BackendId {
    /// Stable string identifier for logging.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::VideoToolbox => "videotoolbox",
            Self::Nvenc => "nvenc",
            Self::Vaapi => "vaapi",
            Self::MediaCodec => "mediacodec",
            Self::MediaFoundation => "media-foundation",
        }
    }
}

/// One encoder or decoder the host can open via a specific backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecCapability {
    /// Video codec family.
    pub codec: CodecId,
    /// Platform engine that provides this capability.
    pub backend: BackendId,
    /// Encode or decode.
    pub direction: Direction,
    /// Profiles supported on this path.
    pub profiles: Vec<Profile>,
    /// Maximum picture width in pixels (inclusive).
    pub max_width: u32,
    /// Maximum picture height in pixels (inclusive).
    pub max_height: u32,
    /// Bitstream layouts this backend accepts or emits.
    pub bitstream_formats: Vec<BitstreamFormat>,
    /// Whether the backend is tuned for real-time / low-latency use.
    pub low_latency: bool,
}

impl CodecCapability {
    /// Returns whether this capability supports `profile` at `width × height`.
    #[must_use]
    pub fn supports(&self, profile: Profile, width: u32, height: u32) -> bool {
        profile.codec() == self.codec
            && self.profiles.contains(&profile)
            && width <= self.max_width
            && height <= self.max_height
    }

    /// Builder for backend crates registering capabilities.
    #[must_use]
    pub fn builder(
        codec: CodecId,
        backend: BackendId,
        direction: Direction,
    ) -> CodecCapabilityBuilder {
        CodecCapabilityBuilder {
            codec,
            backend,
            direction,
            profiles: Vec::new(),
            max_width: 3840,
            max_height: 2160,
            bitstream_formats: BitstreamFormat::for_codec(codec).to_vec(),
            low_latency: true,
        }
    }
}

/// Fluent builder for [`CodecCapability`].
#[derive(Debug, Clone)]
pub struct CodecCapabilityBuilder {
    codec: CodecId,
    backend: BackendId,
    direction: Direction,
    profiles: Vec<Profile>,
    max_width: u32,
    max_height: u32,
    bitstream_formats: Vec<BitstreamFormat>,
    low_latency: bool,
}

impl CodecCapabilityBuilder {
    /// Adds a supported profile.
    #[must_use]
    pub fn profile(mut self, profile: Profile) -> Self {
        if !self.profiles.contains(&profile) {
            self.profiles.push(profile);
        }
        self
    }

    /// Sets maximum picture dimensions.
    #[must_use]
    pub const fn max_resolution(mut self, width: u32, height: u32) -> Self {
        self.max_width = width;
        self.max_height = height;
        self
    }

    /// Overrides supported bitstream formats.
    #[must_use]
    pub fn bitstream_formats(mut self, formats: Vec<BitstreamFormat>) -> Self {
        self.bitstream_formats = formats;
        self
    }

    /// Marks whether this path is low-latency capable.
    #[must_use]
    pub const fn low_latency(mut self, low_latency: bool) -> Self {
        self.low_latency = low_latency;
        self
    }

    /// Builds the capability descriptor.
    #[must_use]
    pub fn build(self) -> CodecCapability {
        CodecCapability {
            codec: self.codec,
            backend: self.backend,
            direction: self.direction,
            profiles: self.profiles,
            max_width: self.max_width,
            max_height: self.max_height,
            bitstream_formats: self.bitstream_formats,
            low_latency: self.low_latency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_supports_profile_and_size() {
        let cap = CodecCapability::builder(CodecId::H264, BackendId::Vaapi, Direction::Encode)
            .profile(Profile::H264Main)
            .max_resolution(1920, 1080)
            .build();

        assert!(cap.supports(Profile::H264Main, 1280, 720));
        assert!(!cap.supports(Profile::HevcMain, 1280, 720));
        assert!(!cap.supports(Profile::H264Main, 3840, 2160));
    }

    #[test]
    fn backend_preference_order() {
        assert!(BackendId::VideoToolbox < BackendId::Vaapi);
        assert!(BackendId::Nvenc < BackendId::Vaapi);
    }
}
