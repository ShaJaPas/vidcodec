//! H.264 hardware encoder via VA-API.

use core::mem;
use core::time::Duration;

use bytes::Bytes;
use vaapi_sys::{
    VA_RC_CBR, VABufferType_VAEncCodedBufferType, VABufferType_VAEncPictureParameterBufferType,
    VABufferType_VAEncSequenceParameterBufferType, VABufferType_VAEncSliceParameterBufferType,
    VACodedBufferSegment, VAEncMiscParameterBuffer, VAEncMiscParameterFrameRate,
    VAEncMiscParameterRateControl, VAEncMiscParameterType_VAEncMiscParameterTypeFrameRate,
    VAEncMiscParameterType_VAEncMiscParameterTypeRateControl, VAEncPictureParameterBufferH264,
    VAEncSequenceParameterBufferH264, VAEncSliceParameterBufferH264, vaBeginPicture, vaEndPicture,
    vaRenderPicture, vaSyncSurface,
};
use vidcodec_core::{
    BitstreamFormat, CodecCapability, EncodedUnit, EncoderConfig, Error, VideoEncoder, VideoFrame,
};

use crate::buffer::Buffer;
use crate::context::Context;
use crate::error::check;
use crate::h264::dpb::{DpbPicture, invalid_va_picture};
use crate::profile::h264_level_idc;
use crate::surface::upload_pixels;
use vidcodec_bitstream::h264::annex_b_to_length_prefixed;

/// H.264 VA-API encoder.
pub(crate) struct H264Encoder {
    ctx: Context,
    capability: CodecCapability,
    config: EncoderConfig,
    surface_index: usize,
    frame_count: u64,
    frame_num: u16,
    force_idr: bool,
    last_pts: Option<Duration>,
    /// Last encoded reference surface for P-frame prediction.
    ref_picture: Option<DpbPicture>,
    parameter_sets: Option<Bytes>,
}

impl H264Encoder {
    /// Opens an H.264 encoder for `cap` using `config`.
    ///
    /// # Errors
    ///
    /// Propagates VA initialization and validation failures.
    pub(crate) fn open(
        ctx: Context,
        capability: CodecCapability,
        config: EncoderConfig,
    ) -> Result<Self, Error> {
        config.validate()?;
        if config.codec != vidcodec_core::CodecId::H264 {
            return Err(Error::InvalidConfig("H264Encoder requires H.264"));
        }
        if !matches!(
            config.bitstream,
            BitstreamFormat::AnnexB | BitstreamFormat::LengthPrefixed
        ) {
            return Err(Error::InvalidConfig(
                "unsupported H.264 bitstream output format",
            ));
        }
        if !capability.supports(config.profile, config.width, config.height) {
            return Err(Error::InvalidConfig("capability does not support config"));
        }

        let mut enc = Self {
            ctx,
            capability,
            config: config.clone(),
            surface_index: 0,
            frame_count: 0,
            frame_num: 0,
            force_idr: true,
            last_pts: None,
            ref_picture: None,
            parameter_sets: None,
        };
        enc.submit_sequence()?;
        enc.submit_rate_control()?;
        enc.submit_frame_rate()?;
        Ok(enc)
    }

    fn submit_sequence(&mut self) -> Result<(), Error> {
        let width = self.config.width;
        let height = self.config.height;
        let mbs_w = width.div_ceil(16) as u16;
        let mbs_h = height.div_ceil(16) as u16;

        // SAFETY: `VAEncSequenceParameterBufferH264` is plain-old-data; zeroed memory is a valid initial state.
        let mut seq = unsafe { mem::zeroed::<VAEncSequenceParameterBufferH264>() };
        seq.seq_parameter_set_id = 0;
        seq.level_idc = h264_level_idc(width, height);
        seq.intra_period = self.config.gop_size;
        seq.intra_idr_period = self.config.gop_size;
        seq.ip_period = 1;
        seq.bits_per_second = self.config.bitrate;
        seq.max_num_ref_frames = 1;
        seq.picture_width_in_mbs = mbs_w;
        seq.picture_height_in_mbs = mbs_h;
        seq.bit_depth_luma_minus8 = 0;
        seq.bit_depth_chroma_minus8 = 0;
        // SAFETY: `seq_fields.bits` is a union field; bitfield setters are safe once the struct is zeroed.
        unsafe {
            seq.seq_fields.bits.set_chroma_format_idc(1);
            seq.seq_fields.bits.set_frame_mbs_only_flag(1);
            seq.seq_fields.bits.set_direct_8x8_inference_flag(1);
        }

        let buf = Buffer::create_typed(
            self.ctx.dpy(),
            self.ctx.id(),
            VABufferType_VAEncSequenceParameterBufferType,
            &seq,
        )?;
        self.render(&[buf.id()])
    }

    fn submit_rate_control(&mut self) -> Result<(), Error> {
        // SAFETY: `VAEncMiscParameterRateControl` is plain-old-data; zeroed memory is a valid initial state.
        let mut rc = unsafe { mem::zeroed::<VAEncMiscParameterRateControl>() };
        rc.bits_per_second = self.config.bitrate;
        rc.target_percentage = 100;
        rc.window_size = 1000;
        rc.initial_qp = 26;
        rc.min_qp = 10;
        rc.max_qp = 51;
        // SAFETY: `rc_flags.bits` is a union field; the bitfield setter is safe once the struct is zeroed.
        unsafe {
            rc.rc_flags.bits.set_mb_rate_control(VA_RC_CBR);
        }

        let header = VAEncMiscParameterBuffer {
            type_: VAEncMiscParameterType_VAEncMiscParameterTypeRateControl,
            // SAFETY: `VAEncMiscParameterBuffer.data` is a fixed-size byte array; zeroed memory is valid.
            data: unsafe { mem::zeroed() },
        };
        let buf = Buffer::create_misc(self.ctx.dpy(), self.ctx.id(), &header, &rc)?;
        self.render(&[buf.id()])
    }

    fn submit_frame_rate(&mut self) -> Result<(), Error> {
        let (num, den) = self.config.frame_rate;
        // SAFETY: `VAEncMiscParameterFrameRate` is plain-old-data; zeroed memory is a valid initial state.
        let mut fr = unsafe { mem::zeroed::<VAEncMiscParameterFrameRate>() };
        fr.framerate = (num << 16) | den;
        let header = VAEncMiscParameterBuffer {
            type_: VAEncMiscParameterType_VAEncMiscParameterTypeFrameRate,
            // SAFETY: `VAEncMiscParameterBuffer.data` is a fixed-size byte array; zeroed memory is valid.
            data: unsafe { mem::zeroed() },
        };
        let buf = Buffer::create_misc(self.ctx.dpy(), self.ctx.id(), &header, &fr)?;
        self.render(&[buf.id()])
    }

    fn render(&self, buffers: &[vaapi_sys::VABufferID]) -> Result<(), Error> {
        check(
            // SAFETY: `self.ctx` is a valid VA context; `buffers` contains valid VA buffer IDs.
            unsafe {
                vaRenderPicture(
                    self.ctx.dpy(),
                    self.ctx.id(),
                    buffers.as_ptr().cast_mut(),
                    buffers.len() as i32,
                )
            },
            "vaRenderPicture",
        )
    }

    fn coded_buffer_size(&self) -> usize {
        (self.config.width as usize)
            .saturating_mul(self.config.height as usize)
            .saturating_mul(2)
            .max(64 * 1024)
    }

    fn read_coded_buffer(&self, coded: &Buffer) -> Result<Vec<u8>, Error> {
        let mapped = coded.map()?;
        // SAFETY: mapped buffer begins with VACodedBufferSegment header.
        let mut segment = unsafe { mapped.read::<VACodedBufferSegment>() };
        let mut out = Vec::new();
        loop {
            if segment.size > 0 && !segment.buf.is_null() {
                // SAFETY: `segment.buf` points to valid data of `segment.size` bytes in the mapped buffer.
                let slice = unsafe {
                    core::slice::from_raw_parts(segment.buf.cast::<u8>(), segment.size as usize)
                };
                out.extend_from_slice(slice);
            }
            if segment.next.is_null() {
                break;
            }
            // SAFETY: `segment.next` is a non-null pointer to a valid `VACodedBufferSegment` in the mapped buffer.
            segment = unsafe { (segment.next as *const VACodedBufferSegment).read() };
        }
        Ok(out)
    }
}

impl VideoEncoder for H264Encoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn reconfigure(&mut self, config: EncoderConfig) -> Result<(), Error> {
        config.validate()?;
        if config.codec != self.config.codec {
            return Err(Error::InvalidConfig("cannot change codec on reconfigure"));
        }
        self.config = config;
        self.ref_picture = None;
        self.parameter_sets = None;
        self.frame_num = 0;
        self.frame_count = 0;
        self.force_idr = true;
        self.submit_sequence()?;
        self.submit_rate_control()?;
        self.submit_frame_rate()
    }

    fn set_bitrate(&mut self, bitrate_bps: u32) {
        self.config.bitrate = bitrate_bps;
        let _ = self.submit_rate_control();
    }

    fn force_keyframe(&mut self) {
        self.force_idr = true;
    }

    fn encode(&mut self, frame: VideoFrame<'_>) -> Result<Vec<EncodedUnit>, Error> {
        frame.validate()?;
        if frame.format != self.config.input_format {
            return Err(Error::InvalidConfig("input pixel format mismatch"));
        }

        let surface = self.ctx.surfaces().get(self.surface_index);
        upload_pixels(
            self.ctx.dpy(),
            self.ctx.id(),
            surface,
            frame.pixels,
            frame.width,
            frame.height,
            frame.format,
        )?;

        let idr = self.force_idr
            || self.frame_count == 0
            || self.frame_count.is_multiple_of(self.config.gop_size as u64);

        if idr {
            self.ref_picture = None;
        }

        let coded = Buffer::create(
            self.ctx.dpy(),
            self.ctx.id(),
            VABufferType_VAEncCodedBufferType,
            self.coded_buffer_size(),
            None,
        )?;

        // SAFETY: `VAEncPictureParameterBufferH264` is plain-old-data; zeroed memory is a valid initial state.
        let mut pic = unsafe { mem::zeroed::<VAEncPictureParameterBufferH264>() };
        pic.CurrPic.picture_id = surface;
        pic.coded_buf = coded.id();
        pic.pic_parameter_set_id = 0;
        pic.seq_parameter_set_id = 0;
        pic.frame_num = self.frame_num;
        pic.pic_init_qp = 26;
        pic.num_ref_idx_l0_active_minus1 = 0;
        let curr_frame_num = u32::from(self.frame_num.saturating_sub(1));
        pic.CurrPic.frame_idx = curr_frame_num;
        // SAFETY: `pic_fields.bits` is a union field; bitfield setters are safe once the struct is zeroed.
        unsafe {
            pic.pic_fields.bits.set_idr_pic_flag(u32::from(idr));
            pic.pic_fields.bits.set_reference_pic_flag(u32::from(!idr));
        }
        if let Some(reference) = self.ref_picture {
            pic.ReferenceFrames[0] = reference.to_va_short_term_reference();
        }
        for slot in pic
            .ReferenceFrames
            .iter_mut()
            .skip(usize::from(self.ref_picture.is_some()))
        {
            *slot = invalid_va_picture();
        }

        let mbs = self.config.width.div_ceil(16) * self.config.height.div_ceil(16);
        // SAFETY: `VAEncSliceParameterBufferH264` is plain-old-data; zeroed memory is a valid initial state.
        let mut slice = unsafe { mem::zeroed::<VAEncSliceParameterBufferH264>() };
        slice.macroblock_address = 0;
        slice.num_macroblocks = mbs;
        slice.slice_type = if idr { 2 } else { 0 }; // I=2, P=0
        slice.pic_parameter_set_id = 0;
        slice.idr_pic_id = 0;
        slice.direct_spatial_mv_pred_flag = 1;
        slice.num_ref_idx_l0_active_minus1 = 0;
        if let Some(reference) = self.ref_picture {
            slice.RefPicList0[0] = reference.to_va_short_term_reference();
        }

        let pic_buf = Buffer::create_typed(
            self.ctx.dpy(),
            self.ctx.id(),
            VABufferType_VAEncPictureParameterBufferType,
            &pic,
        )?;
        let slice_buf = Buffer::create_typed(
            self.ctx.dpy(),
            self.ctx.id(),
            VABufferType_VAEncSliceParameterBufferType,
            &slice,
        )?;

        check(
            // SAFETY: `self.ctx` is a valid VA context and `surface` is a valid render-target surface.
            unsafe { vaBeginPicture(self.ctx.dpy(), self.ctx.id(), surface) },
            "vaBeginPicture",
        )?;
        let render_result = self.render(&[pic_buf.id(), slice_buf.id()]);
        check(
            // SAFETY: `self.ctx` is a valid VA context with a begun picture.
            unsafe { vaEndPicture(self.ctx.dpy(), self.ctx.id()) },
            "vaEndPicture",
        )?;
        render_result?;

        check(
            // SAFETY: `self.ctx.dpy()` is a valid VADisplay and `surface` was used in the encoding.
            unsafe { vaSyncSurface(self.ctx.dpy(), surface) },
            "vaSyncSurface",
        )?;

        let raw = self.read_coded_buffer(&coded)?;
        if idr {
            self.parameter_sets = Some(Bytes::from(
                vidcodec_bitstream::h264::collect_parameter_sets_annex_b(&raw),
            ));
        }
        let payload = match self.config.bitstream {
            BitstreamFormat::AnnexB => raw,
            BitstreamFormat::LengthPrefixed => annex_b_to_length_prefixed(&raw),
            BitstreamFormat::Av1Obu => {
                return Err(Error::InvalidConfig("AV1 output from H.264 encoder"));
            }
        };
        self.force_idr = false;
        self.frame_count += 1;
        self.frame_num = self.frame_num.wrapping_add(1);
        let curr_frame_num = u32::from(self.frame_num.saturating_sub(1));
        self.ref_picture = Some(DpbPicture {
            surface,
            frame_num: curr_frame_num,
            frame_idx: curr_frame_num,
            top_field_order_cnt: self.frame_count as i32 * 2,
            bottom_field_order_cnt: self.frame_count as i32 * 2,
        });
        self.surface_index = (self.surface_index + 1) % self.ctx.surfaces().ids().len();
        self.last_pts = Some(frame.pts);

        Ok(vec![EncodedUnit::new(
            Bytes::from(payload),
            idr,
            frame.pts,
            self.config.bitstream,
        )])
    }

    fn last_encoded_pts(&self) -> Option<Duration> {
        self.last_pts
    }

    fn parameter_sets(&self) -> Option<Bytes> {
        self.parameter_sets.clone()
    }
}
