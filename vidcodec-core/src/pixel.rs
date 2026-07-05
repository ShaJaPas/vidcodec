//! Raw pixel formats and layout helpers.

use crate::Error;

/// Uncompressed video pixel layout accepted by encoders and produced by decoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// 4:2:0 planar Y, U, V (I420 / YUV420P).
    I420,
    /// 4:2:0 semi-planar Y + interleaved UV (NV12).
    Nv12,
}

impl PixelFormat {
    /// Number of bytes required for `width × height` in this format.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidConfig`] when dimensions overflow or are zero.
    pub fn frame_size(self, width: u32, height: u32) -> Result<usize, Error> {
        if width == 0 || height == 0 {
            return Err(Error::InvalidConfig("width and height must be non-zero"));
        }

        let w = width as u64;
        let h = height as u64;
        let y = w
            .checked_mul(h)
            .ok_or(Error::InvalidConfig("dimension overflow"))?;
        let uv = y
            .checked_div(2)
            .ok_or(Error::InvalidConfig("dimension overflow"))?;

        usize::try_from(y + uv).map_err(|_| Error::InvalidConfig("frame too large"))
    }

    /// Validates that `pixels` matches `width`, `height`, and this format.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PixelBufferMismatch`] on size mismatch.
    pub fn validate_buffer(self, pixels: &[u8], width: u32, height: u32) -> Result<(), Error> {
        let expected = self.frame_size(width, height)?;
        if pixels.len() != expected {
            return Err(Error::PixelBufferMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nv12_1080p_size() {
        assert_eq!(PixelFormat::Nv12.frame_size(1920, 1080).unwrap(), 3_110_400);
    }
}
