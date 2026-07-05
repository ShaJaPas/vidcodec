//! Integration tests against local MediaCodec hardware (serial — mutates global
//! registry).

use serial_test::serial;
use vidcodec_core::{BackendId, CodecId, Direction};

#[cfg(target_os = "android")]
#[test]
#[serial]
fn probe_registers_mediacodec_capabilities() {
    vidcodec_core::reset_registry();
    vidcodec_mediacodec::try_register()
        .expect("MediaCodec should be available on this Android device");

    let encode = vidcodec_core::enumerate(Direction::Encode);
    assert!(
        encode
            .iter()
            .any(|c| c.backend == BackendId::MediaCodec && c.codec == CodecId::H264),
        "expected H.264 encode via MediaCodec: {encode:?}"
    );

    let decode = vidcodec_core::enumerate(Direction::Decode);
    assert!(
        decode
            .iter()
            .any(|c| c.backend == BackendId::MediaCodec && c.codec == CodecId::H264),
        "expected H.264 decode via MediaCodec: {decode:?}"
    );

    vidcodec_core::reset_registry();
}
