//! Error types for codec operations.

use alloc::string::String;

/// Codec operation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// No backend registered or none matched the requested capability.
    #[error("no backend available for {codec:?} via {backend:?} ({direction:?})")]
    NoBackend {
        /// Requested video codec.
        codec: crate::CodecId,
        /// Requested platform backend.
        backend: crate::BackendId,
        /// Encode or decode direction.
        direction: crate::Direction,
    },
    /// The host does not expose any encoder/decoder for this direction.
    #[error("no {direction:?} capabilities found on this host")]
    NoCapabilities {
        /// Encode or decode direction.
        direction: crate::Direction,
    },
    /// Encoder/decoder configuration is invalid or unsupported.
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),
    /// Pixel buffer size or layout does not match the declared format and resolution.
    #[error("pixel buffer mismatch: expected {expected} bytes, got {actual}")]
    PixelBufferMismatch {
        /// Expected byte length.
        expected: usize,
        /// Actual byte length.
        actual: usize,
    },
    /// Encoded bitstream could not be parsed or is unsupported.
    #[error("invalid bitstream: {0}")]
    InvalidBitstream(&'static str),
    /// Internal encoder queue is full; caller should retry or drop a frame.
    #[error("encoder backpressure")]
    Backpressure,
    /// Backend returned a platform-specific failure.
    #[error("backend error: {message}")]
    Backend {
        /// Human-readable message from the backend.
        message: String,
    },
    /// Operation is not implemented by this backend build.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

impl Error {
    /// Wraps a backend-specific message.
    #[must_use]
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend {
            message: message.into(),
        }
    }
}
