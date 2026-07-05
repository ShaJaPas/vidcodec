//! [`vidcodec_core::Backend`] implementation for Media Foundation.

use alloc::sync::Arc;

use vidcodec_core::{
    Backend, BackendId, CodecCapability, CodecId, DecoderConfig, Direction, EncoderConfig, Error,
    VideoDecoder, VideoEncoder,
};

use crate::com::ensure_initialized;
use crate::h264::{decode::H264Decoder, encode::H264Encoder};
use crate::probe;

/// Media Foundation backend registered with the vidcodec registry.
pub(crate) struct MfBackend {
    capabilities: Vec<CodecCapability>,
}

impl MfBackend {
    fn new() -> Result<Self, Error> {
        ensure_initialized()?;
        let capabilities = probe::probe()?;
        Ok(Self { capabilities })
    }
}

impl Backend for MfBackend {
    fn id(&self) -> BackendId {
        BackendId::MediaFoundation
    }

    fn enumerate(&self, direction: Direction) -> Vec<CodecCapability> {
        self.capabilities
            .iter()
            .filter(|c| c.direction == direction)
            .cloned()
            .collect()
    }

    fn open_encoder(
        &self,
        cap: &CodecCapability,
        config: EncoderConfig,
    ) -> Result<Box<dyn VideoEncoder>, Error> {
        if cap.backend != BackendId::MediaFoundation || cap.direction != Direction::Encode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => Ok(Box::new(H264Encoder::open(cap.clone(), config)?)),
            _ => Err(Error::NotImplemented("MF encoder for this codec")),
        }
    }

    fn open_decoder(
        &self,
        cap: &CodecCapability,
        config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error> {
        if cap.backend != BackendId::MediaFoundation || cap.direction != Direction::Decode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => Ok(Box::new(H264Decoder::open(cap.clone(), config)?)),
            _ => Err(Error::NotImplemented("MF decoder for this codec")),
        }
    }
}

/// Probes Media Foundation and registers the backend when H.264 MFTs are available.
///
/// # Errors
///
/// Returns [`Error::backend`] when COM/MF cannot be initialized or no H.264 MFTs exist.
pub fn try_register() -> Result<(), Error> {
    let backend = MfBackend::new()?;
    if backend.capabilities.is_empty() {
        return Err(Error::backend(
            "Media Foundation: no H.264 capabilities found",
        ));
    }
    vidcodec_core::register(Arc::new(backend));
    Ok(())
}
