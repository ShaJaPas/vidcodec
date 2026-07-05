//! Integration tests against local VideoToolbox hardware (serial — mutates global registry).

use serial_test::serial;
use vidcodec_core::{BackendId, CodecId, Direction};

#[cfg(target_os = "macos")]
#[test]
#[serial]
fn probe_registers_vt_capabilities() {
    vidcodec_core::reset_registry();
    vidcodec_videotoolbox::try_register().expect("VideoToolbox should be available on this host");

    let encode = vidcodec_core::enumerate(Direction::Encode);
    assert!(
        encode
            .iter()
            .any(|c| c.backend == BackendId::VideoToolbox && c.codec == CodecId::H264),
        "expected H.264 encode via VideoToolbox: {encode:?}"
    );

    let decode = vidcodec_core::enumerate(Direction::Decode);
    assert!(
        decode
            .iter()
            .any(|c| c.backend == BackendId::VideoToolbox && c.codec == CodecId::H264),
        "expected H.264 decode via VideoToolbox: {decode:?}"
    );

    vidcodec_core::reset_registry();
}
