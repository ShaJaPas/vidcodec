//! Media Foundation transform construction.

use vidcodec_core::Error;
use windows::Win32::Media::MediaFoundation::{CMSH264DecoderMFT, CMSH264EncoderMFT, IMFTransform};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};

use crate::error::WinResultExt;

/// Opens the Microsoft H.264 encoder MFT.
pub(crate) fn create_h264_encoder() -> Result<IMFTransform, Error> {
    create_transform_clsid(&CMSH264EncoderMFT)
}

/// Opens the Microsoft H.264 decoder MFT.
pub(crate) fn create_h264_decoder() -> Result<IMFTransform, Error> {
    create_transform_clsid(&CMSH264DecoderMFT)
}

fn create_transform_clsid(clsid: &windows::core::GUID) -> Result<IMFTransform, Error> {
    // SAFETY: CoCreateInstance is safe after COM initialization; it returns a valid COM interface.
    unsafe {
        CoCreateInstance(clsid, None, CLSCTX_INPROC_SERVER)
            .map_err(|err| Error::backend(format!("CoCreateInstance: {err}")))
    }
}

/// Returns stream IDs, defaulting to `(0, 0)` for fixed-stream MFTs.
pub(crate) fn stream_ids(transform: &IMFTransform) -> Result<(u32, u32), Error> {
    let mut input_id = 0u32;
    let mut output_id = 0u32;
    // SAFETY: IMFTransform::GetStreamIDs writes to caller-provided buffers, safe with a valid transform.
    unsafe {
        match transform.GetStreamIDs(
            core::slice::from_mut(&mut input_id),
            core::slice::from_mut(&mut output_id),
        ) {
            Ok(()) => Ok((input_id, output_id)),
            Err(err) if err.code() == windows::core::HRESULT(0x80004001_u32 as i32) => Ok((0, 0)),
            Err(err) => Err(Error::backend(format!("GetStreamIDs: {err}"))),
        }
    }
}

/// Sends the standard begin-streaming notifications.
pub(crate) fn begin_streaming(transform: &IMFTransform) -> Result<(), Error> {
    use windows::Win32::Media::MediaFoundation::{
        MFT_MESSAGE_COMMAND_FLUSH, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
        MFT_MESSAGE_NOTIFY_START_OF_STREAM,
    };

    // SAFETY: IMFTransform::ProcessMessage is safe on a single-threaded MFT with standard message constants.
    unsafe {
        transform
            .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
            .mf()?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .mf()?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .mf()?;
    }
    Ok(())
}

/// Flushes decoder/encoder delay lines.
pub(crate) fn flush_transform(transform: &IMFTransform) -> Result<(), Error> {
    use windows::Win32::Media::MediaFoundation::MFT_MESSAGE_COMMAND_FLUSH;
    // SAFETY: IMFTransform::ProcessMessage with MFT_MESSAGE_COMMAND_FLUSH is safe on a single-threaded MFT.
    unsafe {
        transform
            .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)
            .mf()?
    };
    Ok(())
}
