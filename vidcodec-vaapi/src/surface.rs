//! VA surface pool and NV12 upload/download.

use core::mem::MaybeUninit;
use core::ptr;

use vaapi_sys::{
    VA_FOURCC_I420, VA_FOURCC_NV12, VA_INVALID_SURFACE, VA_LSB_FIRST, VA_RT_FORMAT_YUV420,
    VABufferID, VAContextID, VADisplay, VAImage, VAImageFormat, VASurfaceID, vaCreateImage,
    vaCreateSurfaces, vaDestroyImage, vaDestroySurfaces, vaGetImage, vaMapBuffer, vaPutImage,
    vaUnmapBuffer,
};
use vidcodec_core::{Error, PixelFormat};

use crate::display::VaDisplayHandle;
use crate::error::check;

/// Pool of YUV420 render-target surfaces.
pub(crate) struct SurfacePool {
    display: VaDisplayHandle,
    surfaces: Vec<VASurfaceID>,
}

impl SurfacePool {
    /// Allocates `count` NV12/YUV420 surfaces at `width × height`.
    pub(crate) fn new(
        display: VADisplay,
        width: u32,
        height: u32,
        count: usize,
    ) -> Result<Self, Error> {
        let mut surfaces = vec![VA_INVALID_SURFACE; count];
        let handle = VaDisplayHandle::new(display);
        check(
            // SAFETY: `handle.get()` is a valid VADisplay; `surfaces` has space for `count` surface IDs.
            unsafe {
                vaCreateSurfaces(
                    handle.get(),
                    VA_RT_FORMAT_YUV420,
                    width,
                    height,
                    surfaces.as_mut_ptr(),
                    count as u32,
                    ptr::null_mut(),
                    0,
                )
            },
            "vaCreateSurfaces",
        )?;
        Ok(Self {
            display: handle,
            surfaces,
        })
    }

    #[inline]
    pub(crate) fn ids(&self) -> &[VASurfaceID] {
        &self.surfaces
    }

    #[inline]
    pub(crate) fn get(&self, index: usize) -> VASurfaceID {
        self.surfaces[index]
    }
}

impl Drop for SurfacePool {
    fn drop(&mut self) {
        if !self.surfaces.is_empty() {
            // SAFETY: `self.display.get()` is a valid VADisplay; surfaces were created by `vaCreateSurfaces`.
            unsafe {
                let _ = vaDestroySurfaces(
                    self.display.get(),
                    self.surfaces.as_mut_ptr(),
                    self.surfaces.len() as i32,
                );
            }
        }
    }
}

/// Uploads packed NV12 or I420 pixels into `surface`.
pub(crate) fn upload_pixels(
    display: VADisplay,
    _context: VAContextID,
    surface: VASurfaceID,
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<(), Error> {
    let fourcc = match format {
        PixelFormat::Nv12 => VA_FOURCC_NV12,
        PixelFormat::I420 => VA_FOURCC_I420,
    };

    let mut image = MaybeUninit::<VAImage>::uninit();
    let mut fmt = VAImageFormat {
        fourcc,
        byte_order: VA_LSB_FIRST,
        bits_per_pixel: 12,
        depth: 0,
        red_mask: 0,
        green_mask: 0,
        blue_mask: 0,
        alpha_mask: 0,
        va_reserved: [0; 4],
    };

    check(
        // SAFETY: `display` is a valid VADisplay; `fmt` and `image` are valid output locations.
        unsafe {
            vaCreateImage(
                display,
                &mut fmt,
                width as i32,
                height as i32,
                image.as_mut_ptr(),
            )
        },
        "vaCreateImage",
    )?;
    // SAFETY: `vaCreateImage` initialized `image` on success; `MaybeUninit::assume_init` is sound.
    let image = unsafe { image.assume_init() };

    {
        let mut mapped = map_buffer(display, image.buf)?;
        copy_pixels_to_image(&mut mapped, pixels, width, height, format, &image)?;
    }

    check(
        // SAFETY: `display`, `surface`, and `image.image_id` are valid VA objects.
        unsafe {
            vaPutImage(
                display,
                surface,
                image.image_id,
                0,
                0,
                width,
                height,
                0,
                0,
                width,
                height,
            )
        },
        "vaPutImage",
    )?;

    // SAFETY: `display` is a valid VADisplay and `image.image_id` was created by `vaCreateImage`.
    unsafe {
        let _ = vaDestroyImage(display, image.image_id);
    }
    Ok(())
}

/// Downloads NV12 pixels from `surface` via `vaGetImage`.
pub(crate) fn download_nv12(
    display: VADisplay,
    surface: VASurfaceID,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Error> {
    let mut fmt = VAImageFormat {
        fourcc: VA_FOURCC_NV12,
        byte_order: VA_LSB_FIRST,
        bits_per_pixel: 12,
        depth: 0,
        red_mask: 0,
        green_mask: 0,
        blue_mask: 0,
        alpha_mask: 0,
        va_reserved: [0; 4],
    };
    let mut image = MaybeUninit::<VAImage>::uninit();
    check(
        // SAFETY: `display` is a valid VADisplay; `fmt` and `image` are valid output locations.
        unsafe {
            vaCreateImage(
                display,
                &mut fmt,
                width as i32,
                height as i32,
                image.as_mut_ptr(),
            )
        },
        "vaCreateImage",
    )?;
    // SAFETY: `vaCreateImage` initialized `image` on success; `MaybeUninit::assume_init` is sound.
    let image = unsafe { image.assume_init() };

    check(
        // SAFETY: `display`, `surface`, and `image.image_id` are valid VA objects.
        unsafe { vaGetImage(display, surface, 0, 0, width, height, image.image_id) },
        "vaGetImage",
    )?;

    let expected = PixelFormat::Nv12.frame_size(width, height)?;
    let mut out = vec![0u8; expected];

    {
        let mapped = map_buffer(display, image.buf)?;
        copy_image_to_nv12(&mapped, &mut out, width, height, &image)?;
    }

    // SAFETY: `display` is a valid VADisplay and `image.image_id` was created by `vaCreateImage`.
    unsafe {
        let _ = vaDestroyImage(display, image.image_id);
    }
    Ok(out)
}

fn copy_pixels_to_image(
    mapped: &mut MappedImageBuffer,
    src: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    image: &VAImage,
) -> Result<(), Error> {
    format.validate_buffer(src, width, height)?;
    let dst = mapped.as_mut_slice(image.data_size as usize);
    let w = width as usize;
    let h = height as usize;
    let y_pitch = image.pitches[0] as usize;
    let uv_pitch = image.pitches[1] as usize;
    let y_off = image.offsets[0] as usize;
    let uv_off = image.offsets[1] as usize;

    match format {
        PixelFormat::Nv12 => {
            for row in 0..h {
                let src_start = row * w;
                let dst_start = y_off + row * y_pitch;
                dst[dst_start..dst_start + w].copy_from_slice(&src[src_start..src_start + w]);
            }
            let uv_src_off = w * h;
            let uv_h = h / 2;
            for row in 0..uv_h {
                let src_start = uv_src_off + row * w;
                let dst_start = uv_off + row * uv_pitch;
                dst[dst_start..dst_start + w].copy_from_slice(&src[src_start..src_start + w]);
            }
        }
        PixelFormat::I420 => {
            let u_src = w * h;
            let v_src = u_src + (w / 2) * (h / 2);
            let uv_w = w / 2;
            let uv_h = h / 2;
            for row in 0..h {
                let src_start = row * w;
                let dst_start = y_off + row * y_pitch;
                dst[dst_start..dst_start + w].copy_from_slice(&src[src_start..src_start + w]);
            }
            for row in 0..uv_h {
                for col in 0..uv_w {
                    let u = src[u_src + row * uv_w + col];
                    let v = src[v_src + row * uv_w + col];
                    let dst_idx = uv_off + row * uv_pitch + col * 2;
                    dst[dst_idx] = u;
                    dst[dst_idx + 1] = v;
                }
            }
        }
    }
    Ok(())
}

fn copy_image_to_nv12(
    mapped: &MappedImageBuffer,
    dst: &mut [u8],
    width: u32,
    height: u32,
    image: &VAImage,
) -> Result<(), Error> {
    let src = mapped.as_slice(image.data_size as usize);
    let w = width as usize;
    let h = height as usize;
    let y_pitch = image.pitches[0] as usize;
    let uv_pitch = image.pitches[1] as usize;
    let y_off = image.offsets[0] as usize;
    let uv_off = image.offsets[1] as usize;

    for row in 0..h {
        let src_start = y_off + row * y_pitch;
        let dst_start = row * w;
        dst[dst_start..dst_start + w].copy_from_slice(&src[src_start..src_start + w]);
    }
    let uv_h = h / 2;
    let uv_dst_off = w * h;
    for row in 0..uv_h {
        let src_start = uv_off + row * uv_pitch;
        let dst_start = uv_dst_off + row * w;
        dst[dst_start..dst_start + w].copy_from_slice(&src[src_start..src_start + w]);
    }
    Ok(())
}

fn map_buffer(display: VADisplay, buf_id: VABufferID) -> Result<MappedImageBuffer, Error> {
    let mut ptr = ptr::null_mut();
    check(
        // SAFETY: `display` is a valid VADisplay and `buf_id` is a valid buffer ID from `vaCreateImage`.
        unsafe { vaMapBuffer(display, buf_id, &mut ptr) },
        "vaMapBuffer",
    )?;
    Ok(MappedImageBuffer {
        display,
        buf_id,
        ptr: ptr.cast(),
    })
}

struct MappedImageBuffer {
    display: VADisplay,
    buf_id: VABufferID,
    ptr: *mut u8,
}

impl MappedImageBuffer {
    fn as_slice(&self, len: usize) -> &[u8] {
        // SAFETY: valid for the duration of the mapping.
        unsafe { core::slice::from_raw_parts(self.ptr, len) }
    }

    fn as_mut_slice(&mut self, len: usize) -> &mut [u8] {
        // SAFETY: valid for the duration of the mapping.
        unsafe { core::slice::from_raw_parts_mut(self.ptr, len) }
    }
}

impl Drop for MappedImageBuffer {
    fn drop(&mut self) {
        // SAFETY: `self.display` is a valid VADisplay and `self.buf_id` is a valid mapped buffer.
        unsafe {
            let _ = vaUnmapBuffer(self.display, self.buf_id);
        }
    }
}
