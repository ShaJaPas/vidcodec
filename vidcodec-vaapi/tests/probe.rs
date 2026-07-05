//! Integration tests against local VA hardware (serial — mutates global registry).

use serial_test::serial;
use vidcodec_core::{BackendId, CodecId, Direction};

#[test]
#[serial]
fn probe_registers_vaapi_capabilities() {
    vidcodec_core::reset_registry();
    vidcodec_vaapi::try_register().expect("VA-API should be available on this host");

    let encode = vidcodec_core::enumerate(Direction::Encode);
    assert!(
        encode
            .iter()
            .any(|c| c.backend == BackendId::Vaapi && c.codec == CodecId::H264),
        "expected H.264 encode via VA-API: {encode:?}"
    );

    let decode = vidcodec_core::enumerate(Direction::Decode);
    assert!(
        decode
            .iter()
            .any(|c| c.backend == BackendId::Vaapi && c.codec == CodecId::H264),
        "expected H.264 decode via VA-API: {decode:?}"
    );

    vidcodec_core::reset_registry();
}
