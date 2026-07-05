//! `vaCreateBuffer` / `vaMapBuffer` helpers.

use core::mem::size_of;
use core::ptr;

use vaapi_sys::{
    VABufferID, VABufferType, VAContextID, VADisplay, vaCreateBuffer, vaDestroyBuffer, vaMapBuffer,
    vaUnmapBuffer,
};
use vidcodec_core::Error;

use crate::display::VaDisplayHandle;
use crate::error::check;

/// RAII wrapper around a VA buffer.
pub(crate) struct Buffer {
    display: VaDisplayHandle,
    id: VABufferID,
}

impl Buffer {
    /// Creates a buffer with optional initial `data` (may be `null` for driver-filled buffers).
    ///
    /// # Errors
    ///
    /// Returns [`Error::backend`] when `vaCreateBuffer` fails.
    pub(crate) fn create(
        display: VADisplay,
        context: VAContextID,
        type_: VABufferType,
        size: usize,
        data: Option<&[u8]>,
    ) -> Result<Self, Error> {
        let handle = VaDisplayHandle::new(display);
        let mut id = 0;
        let (ptr, len) = match data {
            Some(bytes) => (bytes.as_ptr().cast_mut().cast(), bytes.len()),
            None => (ptr::null_mut(), size),
        };
        check(
            // SAFETY: `handle.get()` is a valid VADisplay; `ptr` is valid for `len` bytes or null.
            unsafe { vaCreateBuffer(handle.get(), context, type_, len as u32, 1, ptr, &mut id) },
            "vaCreateBuffer",
        )?;
        Ok(Self {
            display: handle,
            id,
        })
    }

    /// Creates a typed parameter buffer from a `Copy` struct.
    pub(crate) fn create_typed<T: Copy>(
        display: VADisplay,
        context: VAContextID,
        type_: VABufferType,
        value: &T,
    ) -> Result<Self, Error> {
        // SAFETY: `value` is a valid `T` reference of size `size_of::<T>()`.
        let bytes = unsafe {
            core::slice::from_raw_parts(ptr::from_ref(value).cast::<u8>(), size_of::<T>())
        };
        Self::create(display, context, type_, size_of::<T>(), Some(bytes))
    }

    /// Creates a misc-parameter buffer (`header` + trailing `payload`).
    pub(crate) fn create_misc<T: Copy>(
        display: VADisplay,
        context: VAContextID,
        header: &vaapi_sys::VAEncMiscParameterBuffer,
        payload: &T,
    ) -> Result<Self, Error> {
        let header_size = size_of::<vaapi_sys::VAEncMiscParameterBuffer>();
        let total = header_size + size_of::<T>();
        let mut bytes = vec![0u8; total];
        // SAFETY: `bytes` has been allocated with `total` bytes; both `header` and `payload` references are valid.
        unsafe {
            ptr::copy_nonoverlapping(
                ptr::from_ref(header).cast(),
                bytes.as_mut_ptr(),
                header_size,
            );
            ptr::copy_nonoverlapping(
                ptr::from_ref(payload).cast(),
                bytes.as_mut_ptr().add(header_size),
                size_of::<T>(),
            );
        }
        Self::create(
            display,
            context,
            vaapi_sys::VABufferType_VAEncMiscParameterBufferType,
            total,
            Some(&bytes),
        )
    }

    #[inline]
    pub(crate) fn id(&self) -> VABufferID {
        self.id
    }

    /// Maps the buffer for CPU read/write.
    ///
    /// # Errors
    ///
    /// Returns [`Error::backend`] when mapping fails.
    pub(crate) fn map(&self) -> Result<MappedBuffer<'_>, Error> {
        let mut ptr = ptr::null_mut();
        check(
            // SAFETY: `self.display.get()` is a valid VADisplay and `self.id` is a valid buffer ID from `vaCreateBuffer`.
            unsafe { vaMapBuffer(self.display.get(), self.id, &mut ptr) },
            "vaMapBuffer",
        )?;
        Ok(MappedBuffer {
            display: self.display.get(),
            id: self.id,
            ptr: ptr.cast(),
            _marker: core::marker::PhantomData,
        })
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: `self.display.get()` is a valid VADisplay and `self.id` is a valid buffer from `vaCreateBuffer`.
        unsafe {
            let _ = vaDestroyBuffer(self.display.get(), self.id);
        }
    }
}

/// Mapped VA buffer view.
pub(crate) struct MappedBuffer<'a> {
    display: VADisplay,
    id: VABufferID,
    ptr: *mut u8,
    _marker: core::marker::PhantomData<&'a ()>,
}

impl MappedBuffer<'_> {
    /// Raw mapped pointer.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Reads a `Copy` value from the start of the mapping.
    ///
    /// # Safety
    ///
    /// Caller must ensure the mapped size is at least `size_of::<T>()` and the layout matches.
    // SAFETY: Caller must ensure the mapped region is at least `size_of::<T>()` bytes and correctly aligned.
    pub(crate) unsafe fn read<T: Copy>(&self) -> T {
        // SAFETY: `self.ptr` is non-null, properly aligned, and points to a valid `T` within the mapped region.
        unsafe { self.ptr.cast::<T>().read() }
    }

    /// Returns a mutable slice view of `len` bytes.
    ///
    /// # Safety
    ///
    /// Caller must ensure `len` does not exceed the mapped region.
    // SAFETY: Caller must ensure `len` does not exceed the mapped buffer size.
    pub(crate) unsafe fn as_mut_slice(&mut self, len: usize) -> &mut [u8] {
        // SAFETY: `self.ptr` is non-null and valid for `len` bytes within the mapped region.
        unsafe { core::slice::from_raw_parts_mut(self.ptr, len) }
    }
}

impl Drop for MappedBuffer<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.display` is a valid VADisplay and `self.id` is a valid mapped buffer.
        unsafe {
            let _ = vaUnmapBuffer(self.display, self.id);
        }
    }
}
