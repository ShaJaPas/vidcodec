//! VA config + context lifecycle.

use alloc::sync::Arc;

use vaapi_sys::{
    VA_ATTRIB_NOT_SUPPORTED, VA_DEC_SLICE_MODE_NORMAL, VA_ENC_PACKED_HEADER_PICTURE,
    VA_ENC_PACKED_HEADER_SEQUENCE, VA_ENC_PACKED_HEADER_SLICE, VA_PROGRESSIVE, VA_RT_FORMAT_YUV420,
    VAConfigAttrib, VAConfigAttribType_VAConfigAttribDecSliceMode,
    VAConfigAttribType_VAConfigAttribEncPackedHeaders, VAConfigAttribType_VAConfigAttribRTFormat,
    VAConfigID, VAContextID, VAEntrypoint, VAProfile, vaCreateConfig, vaCreateContext,
    vaDestroyConfig, vaDestroyContext, vaGetConfigAttributes,
};
use vidcodec_core::Error;

use crate::display::Display;
use crate::error::check;
use crate::profile::entrypoint_direction;
use crate::surface::SurfacePool;
use vidcodec_core::Direction;

/// Initialized VA encode/decode context with render-target surfaces.
pub(crate) struct Context {
    display: Arc<Display>,
    config: VAConfigID,
    context: VAContextID,
    pub profile: VAProfile,
    #[allow(dead_code)]
    pub entrypoint: VAEntrypoint,
    pub width: u32,
    pub height: u32,
    surfaces: SurfacePool,
}

impl Context {
    /// Creates config, surfaces, and context for `profile` / `entrypoint`.
    pub(crate) fn open(
        display: Arc<Display>,
        profile: VAProfile,
        entrypoint: VAEntrypoint,
        width: u32,
        height: u32,
        surface_count: usize,
    ) -> Result<Self, Error> {
        let direction = entrypoint_direction(entrypoint)
            .ok_or_else(|| Error::backend("unsupported VA entrypoint"))?;
        let dpy = display.handle();
        let attribs = config_attribs(dpy, profile, entrypoint, direction)?;

        let mut config = 0;
        check(
            // SAFETY: `dpy` is a valid VADisplay; `attribs` have been properly populated for the profile/entrypoint.
            unsafe {
                vaCreateConfig(
                    dpy,
                    profile,
                    entrypoint,
                    attribs.as_ptr().cast_mut(),
                    attribs.len() as i32,
                    &mut config,
                )
            },
            "vaCreateConfig",
        )?;

        let surfaces = SurfacePool::new(dpy, width, height, surface_count)?;

        let mut context = 0;
        if let Err(e) = check(
            // SAFETY: `dpy` and `config` are valid; `surfaces.ids()` were just created by `vaCreateSurfaces`.
            unsafe {
                vaCreateContext(
                    dpy,
                    config,
                    width as i32,
                    height as i32,
                    VA_PROGRESSIVE as i32,
                    surfaces.ids().as_ptr().cast_mut(),
                    surfaces.ids().len() as i32,
                    &mut context,
                )
            },
            "vaCreateContext",
        ) {
            // SAFETY: `dpy` is a valid VADisplay and `config` was created by `vaCreateConfig`.
            unsafe {
                let _ = vaDestroyConfig(dpy, config);
            }
            return Err(e);
        }

        Ok(Self {
            display,
            config,
            context,
            profile,
            entrypoint,
            width,
            height,
            surfaces,
        })
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn display(&self) -> &Arc<Display> {
        &self.display
    }

    #[inline]
    pub(crate) fn dpy(&self) -> vaapi_sys::VADisplay {
        self.display.handle()
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn config_id(&self) -> VAConfigID {
        self.config
    }

    #[inline]
    pub(crate) fn id(&self) -> VAContextID {
        self.context
    }

    #[inline]
    pub(crate) fn surfaces(&self) -> &SurfacePool {
        &self.surfaces
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        let dpy = self.display.handle();
        // SAFETY: `dpy` is a valid VADisplay; `self.context` and `self.config` were created by `vaCreateContext`/`vaCreateConfig`.
        unsafe {
            let _ = vaDestroyContext(dpy, self.context);
            let _ = vaDestroyConfig(dpy, self.config);
        }
    }
}

fn config_attribs(
    dpy: vaapi_sys::VADisplay,
    profile: VAProfile,
    entrypoint: VAEntrypoint,
    direction: Direction,
) -> Result<Vec<VAConfigAttrib>, Error> {
    let mut attribs = vec![VAConfigAttrib {
        type_: VAConfigAttribType_VAConfigAttribRTFormat,
        value: VA_RT_FORMAT_YUV420,
    }];

    match direction {
        Direction::Encode => {
            let desired = VA_ENC_PACKED_HEADER_SEQUENCE
                | VA_ENC_PACKED_HEADER_PICTURE
                | VA_ENC_PACKED_HEADER_SLICE;
            if let Some(value) = query_attrib(
                dpy,
                profile,
                entrypoint,
                VAConfigAttribType_VAConfigAttribEncPackedHeaders,
                desired,
            )? {
                attribs.push(VAConfigAttrib {
                    type_: VAConfigAttribType_VAConfigAttribEncPackedHeaders,
                    value,
                });
            }
        }
        Direction::Decode => {
            if let Some(value) = query_attrib(
                dpy,
                profile,
                entrypoint,
                VAConfigAttribType_VAConfigAttribDecSliceMode,
                VA_DEC_SLICE_MODE_NORMAL,
            )? {
                attribs.push(VAConfigAttrib {
                    type_: VAConfigAttribType_VAConfigAttribDecSliceMode,
                    value,
                });
            }
        }
    }

    Ok(attribs)
}

fn query_attrib(
    dpy: vaapi_sys::VADisplay,
    profile: VAProfile,
    entrypoint: VAEntrypoint,
    attrib_type: u32,
    desired: u32,
) -> Result<Option<u32>, Error> {
    let mut attrib = VAConfigAttrib {
        type_: attrib_type,
        value: 0,
    };
    check(
        // SAFETY: `dpy` is a valid VADisplay and `attrib` is a valid `VAConfigAttrib` buffer.
        unsafe { vaGetConfigAttributes(dpy, profile, entrypoint, &raw mut attrib, 1) },
        "vaGetConfigAttributes",
    )?;
    if attrib.value == VA_ATTRIB_NOT_SUPPORTED {
        return Ok(None);
    }
    let supported = desired & attrib.value;
    if supported == 0 {
        Ok(None)
    } else {
        Ok(Some(supported))
    }
}
