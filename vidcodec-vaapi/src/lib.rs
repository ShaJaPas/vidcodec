//! Linux VA-API backend for `vidcodec`.

#![deny(missing_docs)]

extern crate alloc;

mod backend;
mod buffer;
mod context;
mod display;
mod error;
mod h264;
mod probe;
mod profile;
mod surface;

pub use backend::try_register;
