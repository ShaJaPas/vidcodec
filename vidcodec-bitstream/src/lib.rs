//! Syntax-only bitstream parsing shared by vidcodec platform backends.
//!
//! Wraps [`oxideav_bitstream`] with vidcodec-oriented helpers (Annex-B ↔ AVCC).
//! Platform crates (`vidcodec-vaapi`, …) depend on this crate — not on
//! `oxideav-bitstream` directly — so the parser can be swapped in one place.

#![deny(missing_docs)]

pub mod h264;

pub use oxideav_bitstream::BitstreamError;
