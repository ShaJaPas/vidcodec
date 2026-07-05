//! VA display initialization (Wayland → X11 → DRM).

use core::ffi::CStr;

use alloc::sync::Arc;
use std::env;
use std::fs::{self, File};
use std::os::fd::AsRawFd;
use std::path::Path;

use vaapi_sys::{
    VADisplay, vaDisplayIsValid, vaGetDisplayDRM, vaInitialize, vaQueryVendorString, vaTerminate,
};
use vidcodec_core::Error;
use wayland_sys::client::{is_lib_available, wayland_client_handle, wl_display};

use crate::error::check;

/// Opened and initialized VA display (shared across encoder/decoder instances).
pub(crate) struct Display {
    display: VaDisplayHandle,
    #[allow(dead_code)]
    connection: DisplayConnection,
    #[allow(dead_code)]
    vendor: String,
}

/// Keeps the native display connection alive for the VA display lifetime.
#[allow(clippy::large_enum_variant, dead_code)]
enum DisplayConnection {
    Wayland(WaylandDisplay),
    X11(Box<X11Display>),
    Drm(File),
}

struct WaylandDisplay(*mut wl_display);

impl Drop for WaylandDisplay {
    fn drop(&mut self) {
        if !self.0.is_null() && is_lib_available() {
            // SAFETY: `self.0` is a non-null `wl_display*` returned by `wl_display_connect` and the library is loaded.
            unsafe {
                (wayland_client_handle().wl_display_disconnect)(self.0);
            }
        }
    }
}

// SAFETY: `WaylandDisplay` only holds a raw `wl_display*`; the libwayland handle guards the library lifetime.
unsafe impl Send for WaylandDisplay {}
// SAFETY: The underlying `wl_display` is only disconnected in `Drop` (single-threaded cleanup).
unsafe impl Sync for WaylandDisplay {}

struct X11Display {
    _lib: x11_dl::xlib::Xlib,
    dpy: *mut x11_dl::xlib::Display,
}

impl Drop for X11Display {
    fn drop(&mut self) {
        if !self.dpy.is_null() {
            // SAFETY: `self.dpy` is a non-null `Display*` from `XOpenDisplay`; the Xlib handle guards the library.
            unsafe {
                (self._lib.XCloseDisplay)(self.dpy);
            }
        }
    }
}

// SAFETY: `X11Display` only holds a raw X11 `Display*`; the `Xlib` handle guards the library lifetime.
unsafe impl Send for X11Display {}
// SAFETY: X11 display is not Sync; VA display is only used from one thread per context in practice.
// The outer `Display` is behind Arc in the backend and encoder instances are Send.
unsafe impl Sync for X11Display {}

/// `VADisplay` is a driver-owned opaque handle; libva documents display objects as
/// shareable across threads when each context is synchronized by the caller.
/// Thread-safe wrapper around opaque `VADisplay`.
pub(crate) struct VaDisplayHandle(VADisplay);

// SAFETY: `VADisplay` is a driver-owned opaque handle; libva documents it as shareable across threads.
unsafe impl Send for VaDisplayHandle {}
// SAFETY: `VADisplay` is a driver-owned opaque handle; libva documents it as shareable across threads.
unsafe impl Sync for VaDisplayHandle {}

impl VaDisplayHandle {
    #[inline]
    pub(crate) fn new(display: VADisplay) -> Self {
        Self(display)
    }

    #[inline]
    pub(crate) fn get(&self) -> VADisplay {
        self.0
    }
}

impl Display {
    /// Opens a VA display, trying Wayland, then X11, then DRM render nodes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::backend`] when no connection path succeeds.
    pub(crate) fn open() -> Result<Arc<Self>, Error> {
        let mut errors = Vec::new();

        if env::var_os("WAYLAND_DISPLAY").is_some() {
            match open_wayland() {
                Ok(display) => return Ok(display),
                Err(e) => errors.push(format!("wayland: {e}")),
            }
        }

        if env::var_os("DISPLAY").is_some() {
            match open_x11() {
                Ok(display) => return Ok(display),
                Err(e) => errors.push(format!("x11: {e}")),
            }
        }

        match open_drm() {
            Ok(display) => return Ok(display),
            Err(e) => errors.push(format!("drm: {e}")),
        }

        Err(Error::backend(format!(
            "failed to open VA display: {}",
            errors.join("; ")
        )))
    }

    /// Underlying `VADisplay` handle.
    #[inline]
    pub(crate) fn handle(&self) -> VADisplay {
        self.display.get()
    }

    /// VA driver vendor string (e.g. Mesa radeonsi).
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn vendor(&self) -> &str {
        &self.vendor
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        // SAFETY: `self.display.get()` is a valid VADisplay that was successfully initialized with `vaInitialize`.
        unsafe {
            let _ = vaTerminate(self.display.get());
        }
    }
}

fn open_wayland() -> Result<Arc<Display>, Error> {
    if !is_lib_available() {
        return Err(Error::backend("libwayland-client.so not available"));
    }
    // SAFETY: `wl_display_connect` returns a connected display or null.
    let wl = unsafe { (wayland_client_handle().wl_display_connect)(core::ptr::null()) };
    if wl.is_null() {
        return Err(Error::backend("wl_display_connect failed"));
    }
    let connection = WaylandDisplay(wl);
    // SAFETY: `wl` outlives the VA display; kept in `DisplayConnection::Wayland`.
    let va_display = unsafe { vaapi_sys::vaGetDisplayWl(wl.cast()) };
    init_display(va_display, DisplayConnection::Wayland(connection))
}

fn open_x11() -> Result<Arc<Display>, Error> {
    let lib = x11_dl::xlib::Xlib::open().map_err(|e| Error::backend(format!("libX11: {e}")))?;
    // SAFETY: `XOpenDisplay` returns an Xlib display or null.
    let x_dpy = unsafe { (lib.XOpenDisplay)(core::ptr::null()) };
    if x_dpy.is_null() {
        return Err(Error::backend("XOpenDisplay failed"));
    }
    let x11 = Box::new(X11Display {
        _lib: lib,
        dpy: x_dpy,
    });
    // SAFETY: `x_dpy` outlives the VA display; kept in `DisplayConnection::X11`.
    let va_display = unsafe { vaapi_sys::vaGetDisplay(x_dpy.cast()) };
    init_display(va_display, DisplayConnection::X11(x11))
}

fn open_drm() -> Result<Arc<Display>, Error> {
    let (file, path) = open_render_node()?;
    let drm_fd = file.as_raw_fd();
    // SAFETY: `drm_fd` valid while `file` is held in `DisplayConnection::Drm`.
    let va_display = unsafe { vaGetDisplayDRM(drm_fd) };
    // SAFETY: `va_display` was just returned by `vaGetDisplayDRM`.
    if unsafe { vaDisplayIsValid(va_display) } == 0 {
        return Err(Error::backend(format!(
            "invalid VA display from {}",
            path.display()
        )));
    }
    init_display(va_display, DisplayConnection::Drm(file))
}

fn init_display(
    va_display: VADisplay,
    connection: DisplayConnection,
) -> Result<Arc<Display>, Error> {
    // SAFETY: `va_display` was just returned by one of the `vaGetDisplay*` functions.
    if unsafe { vaDisplayIsValid(va_display) } == 0 {
        return Err(Error::backend("invalid VA display handle"));
    }

    let mut major = 0;
    let mut minor = 0;
    check(
        // SAFETY: `va_display` has been validated as a valid VA display handle via `vaDisplayIsValid`.
        unsafe { vaInitialize(va_display, &mut major, &mut minor) },
        "vaInitialize",
    )?;

    let vendor = read_driver_vendor(va_display);

    Ok(Arc::new(Display {
        display: VaDisplayHandle::new(va_display),
        connection,
        vendor,
    }))
}

fn open_render_node() -> Result<(File, std::path::PathBuf), Error> {
    let dri = Path::new("/dev/dri");
    let mut nodes = Vec::new();
    for entry in fs::read_dir(dri).map_err(|e| Error::backend(format!("/dev/dri: {e}")))? {
        let entry = entry.map_err(|e| Error::backend(format!("/dev/dri: {e}")))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("renderD") {
            nodes.push(entry.path());
        }
    }
    nodes.sort();

    for path in nodes {
        if let Ok(file) = File::open(&path) {
            return Ok((file, path));
        }
    }

    Err(Error::backend("no usable /dev/dri/renderD* node found"))
}

fn read_driver_vendor(display: VADisplay) -> String {
    // SAFETY: `vaQueryVendorString` returns a static string for the display lifetime.
    unsafe {
        let ptr = vaQueryVendorString(display);
        if ptr.is_null() {
            return String::from("unknown");
        }
        CStr::from_ptr(ptr).to_str().unwrap_or("unknown").to_owned()
    }
}
