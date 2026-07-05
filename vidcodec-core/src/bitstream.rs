//! Encoded bitstream framing conventions.

/// How encoded access units are delimited on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BitstreamFormat {
    /// H.264/HEVC Annex-B (`0x00 0x00 0x00 0x01` start codes).
    #[default]
    AnnexB,
    /// H.264/HEVC AVCC-style length-prefixed NAL units (4-byte big-endian size).
    LengthPrefixed,
    /// AV1 low-overhead bitstream format (OBU sequence).
    Av1Obu,
}

impl BitstreamFormat {
    /// Returns formats valid for a given codec.
    #[must_use]
    pub const fn for_codec(codec: crate::CodecId) -> &'static [Self] {
        match codec {
            crate::CodecId::H264 | crate::CodecId::Hevc => &[Self::AnnexB, Self::LengthPrefixed],
            crate::CodecId::Av1 => &[Self::Av1Obu],
        }
    }
}
