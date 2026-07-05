//! Capability probing via `vaQueryConfigProfiles` / `vaQueryConfigEntrypoints`.

use core::mem::MaybeUninit;
use core::ptr;

use std::collections::HashMap;

use vaapi_sys::{
    VA_INVALID_ID, VA_RT_FORMAT_YUV420, VAConfigAttrib, VAConfigAttribType_VAConfigAttribRTFormat,
    VAConfigID, VAEntrypoint, VAProfile, vaCreateConfig, vaDestroyConfig, vaMaxNumEntrypoints,
    vaMaxNumProfiles, vaQueryConfigEntrypoints, vaQueryConfigProfiles, vaQuerySurfaceAttributes,
};
use vidcodec_core::{BackendId, BitstreamFormat, CodecCapability, CodecId, Direction, Error};

use crate::display::Display;
use crate::error::check;
use crate::profile::{entrypoint_direction, va_profile_codec, va_profile_to_vidcodec};

/// Probes all H.264 capabilities exposed by `display`.
pub(crate) fn probe(display: &Display) -> Result<Vec<CodecCapability>, Error> {
    let dpy = display.handle();
    // SAFETY: `dpy` is a valid VADisplay from an initialized `Display`.
    let max_profiles = unsafe { vaMaxNumProfiles(dpy) };
    if max_profiles <= 0 {
        return Ok(Vec::new());
    }

    let mut profile_list = vec![0; max_profiles as usize];
    let mut num_profiles = 0;
    check(
        // SAFETY: `dpy` is a valid VADisplay; `profile_list` has sufficient capacity for `max_profiles` entries.
        unsafe { vaQueryConfigProfiles(dpy, profile_list.as_mut_ptr(), &mut num_profiles) },
        "vaQueryConfigProfiles",
    )?;

    // SAFETY: `dpy` is a valid VADisplay from an initialized `Display`.
    let max_entrypoints = unsafe { vaMaxNumEntrypoints(dpy) };
    let mut caps = Vec::new();

    for &va_profile in &profile_list[..num_profiles as usize] {
        let Some(codec) = va_profile_codec(va_profile) else {
            continue;
        };
        let Some(vid_profile) = va_profile_to_vidcodec(va_profile) else {
            continue;
        };

        let mut entrypoint_list = vec![0; max_entrypoints as usize];
        let mut num_entrypoints = 0;
        if check(
            // SAFETY: `dpy` is a valid VADisplay; `entrypoint_list` has sufficient capacity.
            unsafe {
                vaQueryConfigEntrypoints(
                    dpy,
                    va_profile,
                    entrypoint_list.as_mut_ptr(),
                    &mut num_entrypoints,
                )
            },
            "vaQueryConfigEntrypoints",
        )
        .is_err()
        {
            continue;
        }

        for &entrypoint in &entrypoint_list[..num_entrypoints as usize] {
            let Some(direction) = entrypoint_direction(entrypoint) else {
                continue;
            };

            let (max_width, max_height) =
                query_max_resolution(dpy, va_profile, entrypoint).unwrap_or((1920, 1080));

            caps.push(
                CodecCapability::builder(codec, BackendId::Vaapi, direction)
                    .profile(vid_profile)
                    .max_resolution(max_width, max_height)
                    .bitstream_formats(vec![
                        BitstreamFormat::AnnexB,
                        BitstreamFormat::LengthPrefixed,
                    ])
                    .low_latency(true)
                    .build(),
            );
        }
    }

    merge_capabilities(caps)
}

fn query_max_resolution(
    dpy: vaapi_sys::VADisplay,
    profile: VAProfile,
    entrypoint: VAEntrypoint,
) -> Option<(u32, u32)> {
    let mut config = VA_INVALID_ID;
    let mut rt_attrib = VAConfigAttrib {
        type_: VAConfigAttribType_VAConfigAttribRTFormat,
        value: VA_RT_FORMAT_YUV420,
    };
    if check(
        // SAFETY: `dpy` is a valid VADisplay; `rt_attrib` and `config` are valid output locations.
        unsafe { vaCreateConfig(dpy, profile, entrypoint, &raw mut rt_attrib, 1, &mut config) },
        "vaCreateConfig",
    )
    .is_err()
    {
        return None;
    }

    let result = query_surface_max(dpy, config);
    // SAFETY: `dpy` is a valid VADisplay and `config` was created by `vaCreateConfig`.
    unsafe {
        let _ = vaDestroyConfig(dpy, config);
    }
    result
}

fn query_surface_max(dpy: vaapi_sys::VADisplay, config: VAConfigID) -> Option<(u32, u32)> {
    use vaapi_sys::{
        VASurfaceAttrib, VASurfaceAttribType_VASurfaceAttribMaxHeight,
        VASurfaceAttribType_VASurfaceAttribMaxWidth,
    };

    let mut num_attribs = 0u32;
    // SAFETY: `dpy` is a valid VADisplay and `config` is a valid VAConfigID; passing null to query count.
    let status =
        unsafe { vaQuerySurfaceAttributes(dpy, config, ptr::null_mut(), &mut num_attribs) };
    if !vaapi_sys::status_ok(status) || num_attribs == 0 {
        return None;
    }

    let mut attribs: Vec<VASurfaceAttrib> = (0..num_attribs)
        .map(|_| {
            // SAFETY: `VASurfaceAttrib` is plain-old-data; `MaybeUninit::zeroed().assume_init()` is safe.
            unsafe { MaybeUninit::zeroed().assume_init() }
        })
        .collect();

    if check(
        // SAFETY: `dpy` is a valid VADisplay; `config` is valid; `attribs` has space for `num_attribs` entries.
        unsafe { vaQuerySurfaceAttributes(dpy, config, attribs.as_mut_ptr(), &mut num_attribs) },
        "vaQuerySurfaceAttributes",
    )
    .is_err()
    {
        return None;
    }

    let mut max_width = 1920u32;
    let mut max_height = 1080u32;
    for attr in &attribs[..num_attribs as usize] {
        // SAFETY: MaxWidth/MaxHeight attribs store integer in `value.i`.
        let value = unsafe { attr.value.value.i as u32 };
        if attr.type_ == VASurfaceAttribType_VASurfaceAttribMaxWidth {
            max_width = value;
        } else if attr.type_ == VASurfaceAttribType_VASurfaceAttribMaxHeight {
            max_height = value;
        }
    }
    Some((max_width, max_height))
}

/// Merges duplicate codec/direction entries into one capability with combined profiles.
fn merge_capabilities(caps: Vec<CodecCapability>) -> Result<Vec<CodecCapability>, Error> {
    let mut map: HashMap<(CodecId, Direction), CodecCapability> = HashMap::new();

    for cap in caps {
        let key = (cap.codec, cap.direction);
        map.entry(key)
            .and_modify(|existing| merge_into(existing, &cap))
            .or_insert(cap);
    }

    Ok(map.into_values().collect())
}

fn merge_into(dst: &mut CodecCapability, src: &CodecCapability) {
    for profile in &src.profiles {
        if !dst.profiles.contains(profile) {
            dst.profiles.push(*profile);
        }
    }
    dst.max_width = dst.max_width.max(src.max_width);
    dst.max_height = dst.max_height.max(src.max_height);
}
