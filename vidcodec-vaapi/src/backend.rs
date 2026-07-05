//! [`vidcodec_core::Backend`] implementation.

use alloc::sync::Arc;

use vaapi_sys::{
    VAEntrypoint, VAEntrypoint_VAEntrypointVLD, VAProfile, vaMaxNumEntrypoints,
    vaQueryConfigEntrypoints,
};
use vidcodec_core::{
    Backend, BackendId, CodecCapability, CodecId, DecoderConfig, Direction, EncoderConfig, Error,
    VideoDecoder, VideoEncoder,
};

use crate::context::Context;
use crate::display::Display;
use crate::error::check;
use crate::h264::{decode::H264Decoder, encode::H264Encoder};
use crate::probe;
use crate::profile::{pick_encode_entrypoint, vidcodec_profile_to_va};

/// VA-API backend registered with the vidcodec registry.
pub(crate) struct VaapiBackend {
    display: Arc<Display>,
    capabilities: Vec<CodecCapability>,
}

impl VaapiBackend {
    fn new(display: Arc<Display>) -> Result<Self, Error> {
        let capabilities = probe::probe(&display)?;
        Ok(Self {
            display,
            capabilities,
        })
    }

    fn resolve_entrypoint(
        &self,
        profile: VAProfile,
        direction: Direction,
    ) -> Result<VAEntrypoint, Error> {
        let dpy = self.display.handle();
        // SAFETY: `dpy` is a valid VADisplay from an initialized `Display`.
        let max = unsafe { vaMaxNumEntrypoints(dpy) };
        let mut list = vec![0; max as usize];
        let mut count = 0;
        check(
            // SAFETY: `dpy` is a valid VADisplay and `list` has sufficient capacity.
            unsafe { vaQueryConfigEntrypoints(dpy, profile, list.as_mut_ptr(), &mut count) },
            "vaQueryConfigEntrypoints",
        )?;
        let entrypoints = &list[..count as usize];
        match direction {
            Direction::Encode => pick_encode_entrypoint(entrypoints)
                .ok_or_else(|| Error::backend("no VA encode entrypoint")),
            Direction::Decode => {
                if entrypoints.contains(&VAEntrypoint_VAEntrypointVLD) {
                    Ok(VAEntrypoint_VAEntrypointVLD)
                } else {
                    Err(Error::backend("no VA decode entrypoint"))
                }
            }
        }
    }
}

impl Backend for VaapiBackend {
    fn id(&self) -> BackendId {
        BackendId::Vaapi
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
        if cap.backend != BackendId::Vaapi || cap.direction != Direction::Encode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => {
                let va_profile = vidcodec_profile_to_va(config.profile)
                    .ok_or(Error::InvalidConfig("unsupported H.264 profile"))?;
                let entrypoint = self.resolve_entrypoint(va_profile, Direction::Encode)?;
                let ctx = Context::open(
                    Arc::clone(&self.display),
                    va_profile,
                    entrypoint,
                    config.width,
                    config.height,
                    4,
                )?;
                Ok(Box::new(H264Encoder::open(ctx, cap.clone(), config)?))
            }
            _ => Err(Error::NotImplemented("VA-API encoder for this codec")),
        }
    }

    fn open_decoder(
        &self,
        cap: &CodecCapability,
        config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error> {
        if cap.backend != BackendId::Vaapi || cap.direction != Direction::Decode {
            return Err(Error::InvalidConfig("capability mismatch"));
        }
        match cap.codec {
            CodecId::H264 => {
                let profile = cap
                    .profiles
                    .first()
                    .copied()
                    .ok_or(Error::InvalidConfig("no H.264 decode profile"))?;
                let va_profile = vidcodec_profile_to_va(profile)
                    .ok_or(Error::InvalidConfig("unsupported H.264 profile"))?;
                let entrypoint = self.resolve_entrypoint(va_profile, Direction::Decode)?;
                Ok(Box::new(H264Decoder::open(
                    Arc::clone(&self.display),
                    cap.clone(),
                    config,
                    va_profile,
                    entrypoint,
                )?))
            }
            _ => Err(Error::NotImplemented("VA-API decoder for this codec")),
        }
    }
}

/// Probes VA-API and registers the backend when hardware is available.
///
/// # Errors
///
/// Returns [`Error::backend`] when the DRM/VA stack cannot be opened. Silent no-op
/// when probing finds zero capabilities (still registers an empty backend).
pub fn try_register() -> Result<(), Error> {
    let display = Display::open()?;
    let backend = VaapiBackend::new(display)?;
    if backend.capabilities.is_empty() {
        return Err(Error::backend("VA-API: no H.264 capabilities found"));
    }
    vidcodec_core::register(Arc::new(backend));
    Ok(())
}
