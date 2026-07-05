//! Video codec identifiers and profiles.

/// Supported video codec families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CodecId {
    /// H.264 / AVC.
    H264,
    /// H.265 / HEVC.
    Hevc,
    /// AV1.
    Av1,
}

impl CodecId {
    /// MIME-style type string used in signaling (e.g. `"video/avc"`).
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::H264 => "video/avc",
            Self::Hevc => "video/hevc",
            Self::Av1 => "video/av01",
        }
    }

    /// Short wire name used in session descriptions (e.g. `"h264"`).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::Hevc => "hevc",
            Self::Av1 => "av1",
        }
    }

    /// Parses a wire/signaling name (`h264`, `avc`, `hevc`, `h265`, `av1`, …).
    #[must_use]
    pub fn parse_wire_name(name: &str) -> Option<Self> {
        if matches_ignore_ascii_case(name, &["h264", "avc", "h.264"]) {
            Some(Self::H264)
        } else if matches_ignore_ascii_case(name, &["hevc", "h265", "h.265"]) {
            Some(Self::Hevc)
        } else if matches_ignore_ascii_case(name, &["av1", "av01"]) {
            Some(Self::Av1)
        } else {
            None
        }
    }
}

fn matches_ignore_ascii_case(name: &str, options: &[&str]) -> bool {
    options
        .iter()
        .any(|option| name.eq_ignore_ascii_case(option))
}

/// Encode or decode direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Compression (raw frames → encoded units).
    Encode,
    /// Decompression (encoded units → raw frames).
    Decode,
}

/// Codec profile negotiated for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    /// H.264 Baseline.
    H264Baseline,
    /// H.264 Main.
    H264Main,
    /// H.264 High.
    H264High,
    /// H.264 Constrained Baseline.
    H264ConstrainedBaseline,
    /// HEVC Main (8-bit).
    HevcMain,
    /// HEVC Main 10 (10-bit).
    HevcMain10,
    /// AV1 Main.
    Av1Main,
    /// AV1 High.
    Av1High,
    /// AV1 Professional.
    Av1Professional,
}

impl Profile {
    /// Returns the owning codec family.
    #[must_use]
    pub const fn codec(self) -> CodecId {
        match self {
            Self::H264Baseline
            | Self::H264Main
            | Self::H264High
            | Self::H264ConstrainedBaseline => CodecId::H264,
            Self::HevcMain | Self::HevcMain10 => CodecId::Hevc,
            Self::Av1Main | Self::Av1High | Self::Av1Professional => CodecId::Av1,
        }
    }

    /// Stable wire name for signaling.
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::H264Baseline => "baseline",
            Self::H264Main => "main",
            Self::H264High => "high",
            Self::H264ConstrainedBaseline => "constrained_baseline",
            Self::HevcMain => "main",
            Self::HevcMain10 => "main10",
            Self::Av1Main => "main",
            Self::Av1High => "high",
            Self::Av1Professional => "professional",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wire_names() {
        assert_eq!(CodecId::parse_wire_name("H264"), Some(CodecId::H264));
        assert_eq!(CodecId::parse_wire_name("av01"), Some(CodecId::Av1));
        assert_eq!(CodecId::parse_wire_name("vp9"), None);
    }

    #[test]
    fn profile_codec_family() {
        assert_eq!(Profile::HevcMain10.codec(), CodecId::Hevc);
    }
}
