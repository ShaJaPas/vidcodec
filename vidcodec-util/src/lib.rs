//! Shared utilities for vidcodec hardware backends.
//!
//! Provides pixel format conversion helpers, H.264 profile defaults,
//! and other common code shared across backends (VA-API, NVENC, MF,
//! VideoToolbox, MediaCodec).

pub mod pixel;
pub mod profile;
