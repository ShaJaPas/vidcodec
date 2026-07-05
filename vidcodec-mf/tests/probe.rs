//! Integration tests against local Media Foundation hardware (serial — mutates global registry).

use serial_test::serial;
use vidcodec_core::{BackendId, CodecId, Direction};

#[cfg(windows)]
#[test]
#[serial]
fn probe_registers_mf_capabilities() {
    vidcodec_core::reset_registry();
    vidcodec_mf::try_register().expect("Media Foundation should be available on this host");

    let encode = vidcodec_core::enumerate(Direction::Encode);
    assert!(
        encode
            .iter()
            .any(|c| c.backend == BackendId::MediaFoundation && c.codec == CodecId::H264),
        "expected H.264 encode via Media Foundation: {encode:?}"
    );

    let decode = vidcodec_core::enumerate(Direction::Decode);
    assert!(
        decode
            .iter()
            .any(|c| c.backend == BackendId::MediaFoundation && c.codec == CodecId::H264),
        "expected H.264 decode via Media Foundation: {decode:?}"
    );

    vidcodec_core::reset_registry();
}
