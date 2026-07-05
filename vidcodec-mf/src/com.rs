//! COM and Media Foundation process-wide initialization.

use std::sync::OnceLock;

use vidcodec_core::Error;
use windows::Win32::Media::MediaFoundation::MFStartup;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

static MF_INIT: OnceLock<Result<(), Error>> = OnceLock::new();

/// Ensures COM and Media Foundation are initialized once per process.
pub(crate) fn ensure_initialized() -> Result<(), Error> {
    MF_INIT
        .get_or_init(|| {
            // SAFETY: per-thread COM init; callers must not pair with CoUninitialize while backends live.
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(|err| Error::backend(format!("CoInitializeEx: {err}")))?;
                MFStartup(windows::Win32::Media::MediaFoundation::MF_VERSION, 0)
                    .map_err(|err| Error::backend(format!("MFStartup: {err}")))?;
            }
            Ok(())
        })
        .clone()
}
