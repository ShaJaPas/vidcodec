//! Hardware-only video codec facade for real-time applications.

#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

extern crate alloc;

use std::sync::Once;

pub use vidcodec_core::{
    Backend, BackendId, BitstreamFormat, CodecCapability, CodecCapabilityBuilder, CodecId,
    DecodedFrame, DecoderConfig, Direction, EncodedUnit, EncoderConfig, Error, PixelFormat,
    Profile, VideoDecoder, VideoEncoder, VideoFrame, register,
};
pub use vidcodec_core::{
    backend, bitstream, capability, codec, decoder, encoder, error, frame, pixel,
};

fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        #[cfg(target_os = "linux")]
        {
            let _ = vidcodec_vaapi::try_register();
        }
        #[cfg(target_os = "android")]
        {
            let _ = vidcodec_mediacodec::try_register();
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            let _ = vidcodec_videotoolbox::try_register();
        }
        #[cfg(windows)]
        {
            let _ = vidcodec_mf::try_register();
        }
        #[cfg(feature = "nvenc")]
        {
            let _ = vidcodec_nvenc::try_register();
        }
    });
}

/// Lists all encode or decode capabilities exposed by registered backends.
///
/// Results are sorted by [`BackendId`] (declaration order = preference),
/// then by [`CodecId`].
#[must_use]
pub fn enumerate(direction: Direction) -> Vec<CodecCapability> {
    init();
    vidcodec_core::enumerate(direction)
}

/// Lists capabilities for one codec family and direction.
#[must_use]
pub fn enumerate_codec(codec: CodecId, direction: Direction) -> Vec<CodecCapability> {
    init();
    vidcodec_core::enumerate_codec(codec, direction)
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
    init();
    vidcodec_core::open_encoder(cap, config)
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
    init();
    vidcodec_core::open_decoder(cap, config)
}

/// Clears all registered backends. Intended for tests.
#[doc(hidden)]
pub fn reset_registry() {
    init();
    vidcodec_core::reset_registry();
}
