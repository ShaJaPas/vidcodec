//! Raw and encoded video frame types.

use core::time::Duration;

use bytes::Bytes;

use crate::{BitstreamFormat, Error, PixelFormat};

/// Uncompressed input to a [`crate::VideoEncoder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoFrame<'a> {
    /// Packed pixel bytes in [`Self::format`].
    pub pixels: &'a [u8],
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Pixel layout of [`Self::pixels`].
    pub format: PixelFormat,
    /// Presentation timestamp relative to the stream origin.
    pub pts: Duration,
}

impl<'a> VideoFrame<'a> {
    /// Validates buffer size against format and dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PixelBufferMismatch`] when the slice length is wrong.
    pub fn validate(&self) -> Result<(), Error> {
        self.format
            .validate_buffer(self.pixels, self.width, self.height)
    }
}

/// One compressed access unit from an encoder (or wire input to a decoder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedUnit {
    /// Compressed bitstream bytes.
    pub data: Bytes,
    /// `true` when the unit is a keyframe / sync sample (IDR, AV1 key frame, …).
    pub is_keyframe: bool,
    /// Presentation timestamp.
    pub pts: Duration,
    /// Decode timestamp when it differs from [`Self::pts`].
    pub dts: Option<Duration>,
    /// How [`Self::data`] is framed.
    pub bitstream: BitstreamFormat,
}

impl EncodedUnit {
    /// Creates a unit with `dts = None`.
    #[must_use]
    pub fn new(
        data: impl Into<Bytes>,
        is_keyframe: bool,
        pts: Duration,
        bitstream: BitstreamFormat,
    ) -> Self {
        Self {
            data: data.into(),
            is_keyframe,
            pts,
            dts: None,
            bitstream,
        }
    }
}

/// Uncompressed output from a [`crate::VideoDecoder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    /// Packed pixel bytes in [`Self::format`].
    pub pixels: Bytes,
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Pixel layout of [`Self::pixels`].
    pub format: PixelFormat,
    /// Presentation timestamp carried from the bitstream.
    pub pts: Duration,
}
