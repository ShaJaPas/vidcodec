//! Core types and traits for `vidcodec` hardware video codec backends.

#![deny(missing_docs)]

extern crate alloc;

mod registry;

pub mod backend;
pub mod bitstream;
pub mod capability;
pub mod codec;
pub mod decoder;
pub mod encoder;
pub mod error;
pub mod frame;
pub mod pixel;
pub mod probe;

pub use backend::{Backend, register};
pub use bitstream::BitstreamFormat;
pub use capability::{BackendId, CodecCapability, CodecCapabilityBuilder};
pub use codec::{CodecId, Direction, Profile};
pub use decoder::{DecoderConfig, VideoDecoder};
pub use encoder::{EncoderConfig, VideoEncoder};
pub use error::Error;
pub use frame::{DecodedFrame, EncodedUnit, VideoFrame};
pub use pixel::PixelFormat;
pub use probe::{enumerate, enumerate_codec, open_decoder, open_encoder, reset_registry};
