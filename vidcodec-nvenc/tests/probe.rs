//! Integration tests against local NVIDIA hardware (serial — mutates global registry).

use serial_test::serial;
use vidcodec_core::{BackendId, CodecId, Direction};

#[test]
#[serial]
fn probe_registers_nvenc_capabilities() {
    vidcodec_core::reset_registry();
    vidcodec_nvenc::try_register().expect("NVENC/NVDEC should be available on this host");

    let encode = vidcodec_core::enumerate(Direction::Encode);
    assert!(
        encode
            .iter()
            .any(|c| c.backend == BackendId::Nvenc && c.codec == CodecId::H264),
        "expected H.264 encode via NVENC: {encode:?}"
    );

    let decode = vidcodec_core::enumerate(Direction::Decode);
    assert!(
        decode
            .iter()
            .any(|c| c.backend == BackendId::Nvenc && c.codec == CodecId::H264),
        "expected H.264 decode via NVDEC: {decode:?}"
    );

    vidcodec_core::reset_registry();
}
