//! Capability probing and codec session opening.

use alloc::vec::Vec;

use crate::registry;
use crate::{
    CodecCapability, CodecId, DecoderConfig, Direction, EncoderConfig, Error, VideoDecoder,
    VideoEncoder,
};

/// Lists all encode or decode capabilities exposed by registered backends.
///
/// Results are sorted by [`crate::BackendId`] (declaration order = preference),
/// then by [`CodecId`].
#[must_use]
pub fn enumerate(direction: Direction) -> Vec<CodecCapability> {
    registry::enumerate(direction)
}

/// Lists capabilities for one codec family and direction.
#[must_use]
pub fn enumerate_codec(codec: CodecId, direction: Direction) -> Vec<CodecCapability> {
    registry::enumerate_codec(codec, direction)
}

/// Opens an encoder for the given capability descriptor.
///
/// # Errors
///
/// Returns [`Error::InvalidConfig`] or [`Error::NoBackend`] when opening fails.
pub fn open_encoder(
    cap: &CodecCapability,
    config: EncoderConfig,
) -> Result<Box<dyn VideoEncoder>, Error> {
    if cap.direction != Direction::Encode {
        return Err(Error::InvalidConfig("capability is not an encoder"));
    }
    if config.codec != cap.codec {
        return Err(Error::InvalidConfig(
            "config codec does not match capability",
        ));
    }
    registry::open_encoder(cap, config)
}

/// Opens a decoder for the given capability descriptor.
///
/// # Errors
///
/// Returns [`Error::InvalidConfig`] or [`Error::NoBackend`] when opening fails.
pub fn open_decoder(
    cap: &CodecCapability,
    config: DecoderConfig,
) -> Result<Box<dyn VideoDecoder>, Error> {
    if cap.direction != Direction::Decode {
        return Err(Error::InvalidConfig("capability is not a decoder"));
    }
    if config.codec != cap.codec {
        return Err(Error::InvalidConfig(
            "config codec does not match capability",
        ));
    }
    registry::open_decoder(cap, config)
}

/// Clears all registered backends. Intended for tests.
#[doc(hidden)]
pub fn reset_registry() {
    registry::clear_for_tests();
}
