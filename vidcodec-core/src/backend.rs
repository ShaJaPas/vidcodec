//! Backend registration for platform crates.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::{
    CodecCapability, DecoderConfig, Direction, EncoderConfig, Error, VideoDecoder, VideoEncoder,
};

use crate::registry;

/// Platform backend that enumerates and opens codec instances.
pub trait Backend: Send + Sync {
    /// Stable backend identifier.
    fn id(&self) -> crate::BackendId;

    /// Lists capabilities this backend exposes on the current host.
    fn enumerate(&self, direction: Direction) -> Vec<CodecCapability>;

    /// Opens an encoder for `cap` (must match [`Self::enumerate`] output).
    ///
    /// # Errors
    ///
    /// Propagates backend initialization failures.
    fn open_encoder(
        &self,
        cap: &CodecCapability,
        config: EncoderConfig,
    ) -> Result<Box<dyn VideoEncoder>, Error>;

    /// Opens a decoder for `cap`.
    ///
    /// # Errors
    ///
    /// Propagates backend initialization failures.
    fn open_decoder(
        &self,
        cap: &CodecCapability,
        config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error>;
}

/// Registers a platform backend for the process lifetime.
///
/// [`enumerate`](crate::probe::enumerate) returns capabilities sorted by [`BackendId`](crate::BackendId) declaration order
/// (most-preferred backend first), then by [`crate::CodecId`].
pub fn register(backend: Arc<dyn Backend>) {
    registry::register(backend);
}
