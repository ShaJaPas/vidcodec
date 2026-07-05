//! [`vidcodec_core::Backend`] implementation for NVENC.

use alloc::sync::Arc;

use vidcodec_core::{
    Backend, BackendId, CodecCapability, CodecId, DecoderConfig, Direction, EncoderConfig, Error,
    VideoDecoder, VideoEncoder,
};

use crate::device::Device;
use crate::h264::decode::H264Decoder;
use crate::h264::encode::H264Encoder;
use crate::probe;

/// NVENC backend registered with the vidcodec registry.
pub(crate) struct NvencBackend {
    device: Arc<Device>,
    capabilities: Vec<CodecCapability>,
}

impl NvencBackend {
    fn new(device: Arc<Device>) -> Result<Self, Error> {
        let capabilities = probe::probe(&device)?;
        Ok(Self {
            device,
            capabilities,
        })
    }
}

impl Backend for NvencBackend {
    fn id(&self) -> BackendId {
        BackendId::Nvenc
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
        if cap.backend != BackendId::Nvenc || cap.direction != Direction::Encode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => Ok(Box::new(H264Encoder::open(
                &self.device,
                cap.clone(),
                config,
            )?)),
            _ => Err(Error::NotImplemented("NVENC encoder for this codec")),
        }
    }

    fn open_decoder(
        &self,
        cap: &CodecCapability,
        config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error> {
        if cap.backend != BackendId::Nvenc || cap.direction != Direction::Decode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => Ok(Box::new(H264Decoder::open(
                Arc::clone(&self.device),
                cap.clone(),
                config,
            )?)),
            _ => Err(Error::NotImplemented("NVDEC decoder for this codec")),
        }
    }
}

/// Probes NVENC/NVDEC and registers the backend when an NVIDIA GPU is available.
///
/// # Errors
///
/// Returns [`Error::backend`] when CUDA/NVENC cannot be initialized.
pub fn try_register() -> Result<(), Error> {
    let device = Device::open(0)?;
    let backend = NvencBackend::new(device)?;
    if backend.capabilities.is_empty() {
        return Err(Error::backend("NVENC/NVDEC: no capabilities found"));
    }
    vidcodec_core::register(Arc::new(backend));
    Ok(())
}
