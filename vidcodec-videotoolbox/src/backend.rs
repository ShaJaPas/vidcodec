//! [`vidcodec_core::Backend`] implementation for VideoToolbox.

use alloc::sync::Arc;

use vidcodec_core::{
    Backend, BackendId, CodecCapability, CodecId, DecoderConfig, Direction, EncoderConfig, Error,
    VideoDecoder, VideoEncoder,
};

use crate::h264::{decode::H264Decoder, encode::H264Encoder};
use crate::probe;

/// VideoToolbox backend registered with the vidcodec registry.
pub(crate) struct VtBackend {
    capabilities: Vec<CodecCapability>,
}

impl VtBackend {
    fn new() -> Result<Self, Error> {
        let capabilities = probe::probe()?;
        Ok(Self { capabilities })
    }
}

impl Backend for VtBackend {
    fn id(&self) -> BackendId {
        BackendId::VideoToolbox
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
        if cap.backend != BackendId::VideoToolbox || cap.direction != Direction::Encode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => Ok(Box::new(H264Encoder::open(cap.clone(), config)?)),
            _ => Err(Error::NotImplemented("VideoToolbox encoder for this codec")),
        }
    }

    fn open_decoder(
        &self,
        cap: &CodecCapability,
        config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error> {
        if cap.backend != BackendId::VideoToolbox || cap.direction != Direction::Decode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => Ok(Box::new(H264Decoder::open(cap.clone(), config)?)),
            _ => Err(Error::NotImplemented("VideoToolbox decoder for this codec")),
        }
    }
}

/// Probes VideoToolbox and registers the backend when H.264 codecs are available.
///
/// # Errors
///
/// Returns [`Error::backend`] when no H.264 capabilities are found.
pub fn try_register() -> Result<(), Error> {
    let backend = VtBackend::new()?;
    if backend.capabilities.is_empty() {
        return Err(Error::backend("VideoToolbox: no H.264 capabilities found"));
    }
    vidcodec_core::register(Arc::new(backend));
    Ok(())
}
