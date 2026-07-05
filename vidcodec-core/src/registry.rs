//! Global backend registry (populated by platform crates).

use alloc::sync::Arc;
use alloc::vec::Vec;
use std::sync::{Mutex, OnceLock};

use crate::backend::Backend;
use crate::{
    CodecCapability, DecoderConfig, Direction, EncoderConfig, Error, VideoDecoder, VideoEncoder,
};

struct Registry {
    backends: Vec<Arc<dyn Backend>>,
}

impl Registry {
    const fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    fn register(&mut self, backend: Arc<dyn Backend>) {
        self.backends.push(backend);
    }

    fn enumerate(&self, direction: Direction) -> Vec<CodecCapability> {
        let mut caps = Vec::new();
        for backend in &self.backends {
            caps.extend(backend.enumerate(direction));
        }
        sort_capabilities(&mut caps);
        caps
    }

    fn enumerate_codec(&self, codec: crate::CodecId, direction: Direction) -> Vec<CodecCapability> {
        self.enumerate(direction)
            .into_iter()
            .filter(|cap| cap.codec == codec)
            .collect()
    }

    fn open_encoder(
        &self,
        cap: &CodecCapability,
        config: EncoderConfig,
    ) -> Result<Box<dyn VideoEncoder>, Error> {
        config.validate()?;
        for backend in &self.backends {
            if backend.id() == cap.backend {
                return backend.open_encoder(cap, config);
            }
        }
        Err(Error::NoBackend {
            codec: cap.codec,
            backend: cap.backend,
            direction: cap.direction,
        })
    }

    fn open_decoder(
        &self,
        cap: &CodecCapability,
        config: DecoderConfig,
    ) -> Result<Box<dyn VideoDecoder>, Error> {
        config.validate()?;
        for backend in &self.backends {
            if backend.id() == cap.backend {
                return backend.open_decoder(cap, config);
            }
        }
        Err(Error::NoBackend {
            codec: cap.codec,
            backend: cap.backend,
            direction: cap.direction,
        })
    }
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

fn registry() -> std::sync::MutexGuard<'static, Registry> {
    REGISTRY
        .get_or_init(|| Mutex::new(Registry::new()))
        .lock()
        .expect("vidcodec registry mutex poisoned")
}

pub(crate) fn register(backend: Arc<dyn Backend>) {
    registry().register(backend);
}

pub(crate) fn enumerate(direction: Direction) -> Vec<CodecCapability> {
    let guard = registry();
    guard.enumerate(direction)
}

pub(crate) fn enumerate_codec(codec: crate::CodecId, direction: Direction) -> Vec<CodecCapability> {
    let guard = registry();
    guard.enumerate_codec(codec, direction)
}

pub(crate) fn open_encoder(
    cap: &CodecCapability,
    config: EncoderConfig,
) -> Result<Box<dyn VideoEncoder>, Error> {
    let guard = registry();
    guard.open_encoder(cap, config)
}

pub(crate) fn open_decoder(
    cap: &CodecCapability,
    config: DecoderConfig,
) -> Result<Box<dyn VideoDecoder>, Error> {
    let guard = registry();
    guard.open_decoder(cap, config)
}

pub(crate) fn sort_capabilities(caps: &mut [CodecCapability]) {
    caps.sort_by(|left, right| {
        left.backend
            .cmp(&right.backend)
            .then_with(|| left.codec.cmp(&right.codec))
    });
}

pub(crate) fn clear_for_tests() {
    *registry() = Registry::new();
}
