//! H.264 hardware decoder via VA-API.

use core::mem;

use alloc::sync::Arc;

use bytes::Bytes;
use vaapi_sys::{
    VA_SLICE_DATA_FLAG_ALL, VABufferType_VAIQMatrixBufferType,
    VABufferType_VAPictureParameterBufferType, VABufferType_VASliceDataBufferType,
    VABufferType_VASliceParameterBufferType, VAEntrypoint, VAProfile, VASliceParameterBufferH264,
    vaBeginPicture, vaEndPicture, vaRenderPicture, vaSyncSurface,
};
use vidcodec_bitstream::h264::{
    H264Pps, H264SliceVaInfo, H264Sps, NAL_TYPE_IDR, NAL_TYPE_NON_IDR_SLICE, NAL_TYPE_PPS,
    NAL_TYPE_SPS, length_prefixed_to_annex_b, nal_header, parse_pps_nal, parse_slice_for_va,
    parse_sps_nal, split_annex_b,
};
use vidcodec_core::{
    BitstreamFormat, CodecCapability, DecodedFrame, DecoderConfig, EncodedUnit, Error, VideoDecoder,
};

use crate::buffer::Buffer;
use crate::context::Context;
use crate::display::Display;
use crate::error::check;
use crate::h264::dpb::{Dpb, DpbPicture, field_order_counts, invalid_va_picture};
use crate::h264::iq_matrix::default_iq_matrix;
use crate::h264::picture::build_picture_params;
use crate::profile::{sps_to_va_profile, vidcodec_profile_to_va};
use crate::surface::download_nv12;

/// H.264 VA-API decoder.
pub(crate) struct H264Decoder {
    display: Arc<Display>,
    capability: CodecCapability,
    config: DecoderConfig,
    va_profile: VAProfile,
    entrypoint: VAEntrypoint,
    ctx: Option<Context>,
    sps: Option<H264Sps>,
    pps: Option<H264Pps>,
    dpb: Dpb,
    surface_index: usize,
    extradata_loaded: bool,
}

impl H264Decoder {
    /// Opens an H.264 decoder for `cap`.
    pub(crate) fn open(
        display: Arc<Display>,
        capability: CodecCapability,
        config: DecoderConfig,
        va_profile: VAProfile,
        entrypoint: VAEntrypoint,
    ) -> Result<Self, Error> {
        config.validate()?;
        if config.codec != vidcodec_core::CodecId::H264 {
            return Err(Error::InvalidConfig("H264Decoder requires H.264"));
        }
        if !BitstreamFormat::for_codec(config.codec).contains(&config.bitstream) {
            return Err(Error::InvalidConfig("unsupported H.264 bitstream format"));
        }
        let _ = vidcodec_profile_to_va(
            capability
                .profiles
                .first()
                .copied()
                .unwrap_or(vidcodec_core::Profile::H264Main),
        );
        Ok(Self {
            display,
            capability,
            config: config.clone(),
            va_profile,
            entrypoint,
            ctx: None,
            sps: None,
            pps: None,
            dpb: Dpb::default(),
            surface_index: 0,
            extradata_loaded: false,
        })
    }

    fn bootstrap_extradata(&mut self) -> Result<(), Error> {
        if let Some(extradata) = self.config.extradata.clone() {
            self.ingest_parameter_sets(&extradata)?;
        }
        Ok(())
    }

    fn ensure_context(&mut self, sps: &H264Sps) -> Result<(), Error> {
        let width = sps.display_width().max(16);
        let height = sps.display_height().max(16);
        let va_profile = sps_to_va_profile(sps);
        let needs_new = self.ctx.as_ref().is_none_or(|ctx| {
            ctx.width != width || ctx.height != height || ctx.profile != va_profile
        });
        if needs_new {
            self.ctx = Some(Context::open(
                Arc::clone(&self.display),
                va_profile,
                self.entrypoint,
                width,
                height,
                4,
            )?);
            self.surface_index = 0;
            self.dpb.clear();
            self.va_profile = va_profile;
        }
        Ok(())
    }

    fn ingest_parameter_sets(&mut self, annex_b: &[u8]) -> Result<(), Error> {
        for nal in split_annex_b(annex_b) {
            if nal.is_empty() {
                continue;
            }
            let (_, _, nal_type) = nal_header(nal[0]);
            match nal_type {
                NAL_TYPE_SPS => self.sps = Some(parse_sps_nal(nal).map_err(bitstream_err)?),
                NAL_TYPE_PPS => {
                    let _ = self
                        .sps
                        .as_ref()
                        .ok_or(Error::InvalidBitstream("PPS before SPS"))?;
                    self.pps = Some(parse_pps_nal(nal).map_err(bitstream_err)?);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn decode_slices(
        &mut self,
        annex_b: &[u8],
        pts: core::time::Duration,
    ) -> Result<Vec<DecodedFrame>, Error> {
        let sps = self
            .sps
            .as_ref()
            .ok_or(Error::InvalidBitstream("missing SPS"))?
            .clone();
        let pps = self
            .pps
            .as_ref()
            .ok_or(Error::InvalidBitstream("missing PPS"))?
            .clone();
        self.ensure_context(&sps)?;
        let ctx = self.ctx.as_ref().expect("context initialized");
        let width = ctx.width;
        let height = ctx.height;
        let surface_index = self.surface_index;

        let mut slice_nals = Vec::new();
        for nal in split_annex_b(annex_b) {
            if nal.is_empty() {
                continue;
            }
            let (_, _, nal_type) = nal_header(nal[0]);
            if matches!(nal_type, NAL_TYPE_IDR | NAL_TYPE_NON_IDR_SLICE) {
                slice_nals.push(nal);
            }
        }
        if slice_nals.is_empty() {
            return Err(Error::InvalidBitstream("access unit has no slice NAL"));
        }

        let first_nal_type = nal_header(slice_nals[0][0]).2;
        let is_idr = first_nal_type == NAL_TYPE_IDR;
        if is_idr {
            self.dpb.clear();
        }

        let first_slice = parse_slice_for_va(slice_nals[0], &sps, &pps).map_err(bitstream_err)?;
        let first_slice_header = &first_slice.header;

        if !is_idr && self.dpb.primary_ref().is_none() {
            return Err(Error::InvalidBitstream("P-slice without reference picture"));
        }

        let (top_poc, bottom_poc) =
            field_order_counts(&sps, &first_slice.header, self.dpb.prev_top_poc());
        let nal_ref_idc = nal_header(slice_nals[0][0]).1;
        let surface = ctx.surfaces().get(surface_index);
        let references: Vec<DpbPicture> = self.dpb.references().to_vec();
        let pic = build_picture_params(
            &sps,
            &pps,
            surface,
            first_slice_header,
            nal_ref_idc,
            top_poc,
            bottom_poc,
            &references,
        );
        let pic_buf = Buffer::create_typed(
            ctx.dpy(),
            ctx.id(),
            VABufferType_VAPictureParameterBufferType,
            &pic,
        )?;
        let iq = default_iq_matrix();
        let iq_buf =
            Buffer::create_typed(ctx.dpy(), ctx.id(), VABufferType_VAIQMatrixBufferType, &iq)?;

        check(
            // SAFETY: `ctx` is a valid initialized VA context and `surface` is a valid render-target surface.
            unsafe { vaBeginPicture(ctx.dpy(), ctx.id(), surface) },
            "vaBeginPicture",
        )?;

        let mut render_ids = vec![pic_buf.id(), iq_buf.id()];
        let mut _slice_bufs = Vec::new();
        let mut _data_bufs = Vec::new();
        let _iq_buf = iq_buf;

        for slice in slice_nals {
            let slice_info = parse_slice_for_va(slice, &sps, &pps).map_err(bitstream_err)?;
            let params = build_slice_params(slice, &slice_info, &pps, &references)?;
            let slice_buf = Buffer::create_typed(
                ctx.dpy(),
                ctx.id(),
                VABufferType_VASliceParameterBufferType,
                &params,
            )?;
            let data_buf = Buffer::create(
                ctx.dpy(),
                ctx.id(),
                VABufferType_VASliceDataBufferType,
                slice.len(),
                None,
            )?;
            {
                let mut mapped = data_buf.map()?;
                // SAFETY: buffer was created with `slice.len()` bytes.
                unsafe {
                    mapped.as_mut_slice(slice.len()).copy_from_slice(slice);
                }
            }
            render_ids.push(slice_buf.id());
            render_ids.push(data_buf.id());
            _slice_bufs.push(slice_buf);
            _data_bufs.push(data_buf);
        }

        let status = check(
            // SAFETY: `ctx` is a valid VA context; `render_ids` is a list of valid VA buffer IDs.
            unsafe {
                vaRenderPicture(
                    ctx.dpy(),
                    ctx.id(),
                    render_ids.as_mut_ptr(),
                    render_ids.len() as i32,
                )
            },
            "vaRenderPicture",
        );
        check(
            // SAFETY: `ctx` is a valid VA context with a begun picture.
            unsafe { vaEndPicture(ctx.dpy(), ctx.id()) },
            "vaEndPicture",
        )?;
        status?;

        check(
            // SAFETY: `ctx.dpy()` is a valid VADisplay and `surface` is a valid VASurfaceID used in the current picture.
            unsafe { vaSyncSurface(ctx.dpy(), surface) },
            "vaSyncSurface",
        )?;

        let pixels = download_nv12(ctx.dpy(), surface, width, height)?;

        self.dpb.store(
            DpbPicture {
                surface,
                frame_num: first_slice_header.frame_num,
                frame_idx: first_slice_header.frame_num,
                top_field_order_cnt: top_poc,
                bottom_field_order_cnt: bottom_poc,
            },
            sps.max_num_ref_frames as usize,
        );
        self.surface_index = (self.surface_index + 1) % ctx.surfaces().ids().len();

        Ok(vec![DecodedFrame {
            pixels: Bytes::from(pixels),
            width,
            height,
            format: self.config.output_format,
            pts,
        }])
    }
}

impl VideoDecoder for H264Decoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn decode(&mut self, unit: &EncodedUnit) -> Result<Vec<DecodedFrame>, Error> {
        if !self.extradata_loaded {
            self.bootstrap_extradata()?;
            self.extradata_loaded = true;
        }
        if unit.data.is_empty() {
            return Err(Error::InvalidBitstream("empty access unit"));
        }
        let annex_b = match unit.bitstream {
            BitstreamFormat::AnnexB => unit.data.to_vec(),
            BitstreamFormat::LengthPrefixed => {
                length_prefixed_to_annex_b(&unit.data).map_err(bitstream_err)?
            }
            BitstreamFormat::Av1Obu => {
                return Err(Error::InvalidBitstream("AV1 bitstream in H.264 decoder"));
            }
        };
        self.ingest_parameter_sets(&annex_b)?;
        self.decode_slices(&annex_b, unit.pts)
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.ctx = None;
        self.sps = None;
        self.pps = None;
        self.dpb.clear();
        self.surface_index = 0;
        self.extradata_loaded = false;
        Ok(())
    }
}

fn build_slice_params(
    slice: &[u8],
    slice_info: &H264SliceVaInfo,
    pps: &H264Pps,
    references: &[DpbPicture],
) -> Result<VASliceParameterBufferH264, Error> {
    // SAFETY: `VASliceParameterBufferH264` is a plain-old-data struct; zeroed memory is a valid initial state.
    let mut params = unsafe { mem::zeroed::<VASliceParameterBufferH264>() };
    params.slice_data_size = slice.len() as u32;
    params.slice_data_offset = 0;
    params.slice_data_flag = VA_SLICE_DATA_FLAG_ALL;
    params.slice_data_bit_offset = slice_info.slice_data_bit_offset;
    params.first_mb_in_slice = slice_info.header.first_mb_in_slice as u16;
    params.slice_type = slice_info.header.slice_type;
    params.direct_spatial_mv_pred_flag = 1;
    params.num_ref_idx_l0_active_minus1 = pps.num_ref_idx_l0_default_active_minus1;
    params.num_ref_idx_l1_active_minus1 = pps.num_ref_idx_l1_default_active_minus1;
    params.cabac_init_idc = slice_info.cabac_init_idc;
    params.slice_qp_delta = slice_info.slice_qp_delta;
    params.disable_deblocking_filter_idc = slice_info.disable_deblocking_filter_idc;

    if let Some(reference) = references.last() {
        params.RefPicList0[0] = reference.to_va_short_term_reference();
    }
    for slot in params.RefPicList0.iter_mut().skip(references.len().min(1)) {
        *slot = invalid_va_picture();
    }

    Ok(params)
}

fn bitstream_err(err: vidcodec_bitstream::BitstreamError) -> Error {
    Error::backend(err.to_string())
}
