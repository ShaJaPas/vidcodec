//! Map [`vidcodec_bitstream::h264`] types to VA-API picture parameters.

use core::mem;

use vaapi_sys::VAPictureParameterBufferH264;
use vidcodec_bitstream::h264::{H264Pps, H264SliceHeader, H264Sps};

use crate::h264::dpb::{DpbPicture, current_va_picture, invalid_va_picture};

/// Builds [`VAPictureParameterBufferH264`] for decode.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_picture_params(
    sps: &H264Sps,
    pps: &H264Pps,
    curr_surface: vaapi_sys::VASurfaceID,
    slice_header: &H264SliceHeader,
    nal_ref_idc: u8,
    top_poc: i32,
    bottom_poc: i32,
    references: &[DpbPicture],
) -> VAPictureParameterBufferH264 {
    // SAFETY: `VAPictureParameterBufferH264` is plain-old-data; zeroed memory is a valid initial state.
    let mut pic = unsafe { mem::zeroed::<VAPictureParameterBufferH264>() };
    pic.CurrPic = current_va_picture(curr_surface, slice_header.frame_num, top_poc, bottom_poc);

    for (slot, reference) in pic.ReferenceFrames.iter_mut().zip(references.iter()) {
        *slot = reference.to_va_short_term_reference();
    }
    for slot in pic.ReferenceFrames.iter_mut().skip(references.len()) {
        *slot = invalid_va_picture();
    }

    pic.picture_width_in_mbs_minus1 = sps.pic_width_in_mbs_minus1 as u16;
    pic.picture_height_in_mbs_minus1 = picture_height_in_mbs_minus1(sps);
    pic.bit_depth_luma_minus8 = sps.bit_depth_luma_minus8;
    pic.bit_depth_chroma_minus8 = sps.bit_depth_chroma_minus8;
    pic.num_ref_frames = sps.max_num_ref_frames.min(255) as u8;
    pic.num_slice_groups_minus1 = pps.num_slice_groups_minus1.min(255) as u8;
    pic.slice_group_map_type = 0;
    pic.slice_group_change_rate_minus1 = 0;
    pic.pic_init_qp_minus26 = pps.pic_init_qp_minus26 as i8;
    pic.pic_init_qs_minus26 = pps.pic_init_qs_minus26 as i8;
    pic.chroma_qp_index_offset = pps.chroma_qp_index_offset as i8;
    pic.second_chroma_qp_index_offset = pps.second_chroma_qp_index_offset as i8;
    pic.frame_num = slice_header.frame_num as u16;

    // SAFETY: `seq_fields.bits` and `pic_fields.bits` are union bitfields; safe once the struct is zeroed.
    unsafe {
        pic.seq_fields
            .bits
            .set_chroma_format_idc(u32::from(sps.chroma_format_idc));
        pic.seq_fields.bits.set_residual_colour_transform_flag(0);
        pic.seq_fields
            .bits
            .set_gaps_in_frame_num_value_allowed_flag(u32::from(
                sps.gaps_in_frame_num_value_allowed_flag,
            ));
        pic.seq_fields
            .bits
            .set_frame_mbs_only_flag(u32::from(sps.frame_mbs_only_flag));
        pic.seq_fields
            .bits
            .set_mb_adaptive_frame_field_flag(u32::from(sps.mb_adaptive_frame_field_flag));
        pic.seq_fields
            .bits
            .set_direct_8x8_inference_flag(u32::from(sps.direct_8x8_inference_flag));
        pic.seq_fields
            .bits
            .set_MinLumaBiPredSize8x8(u32::from(sps.level_idc >= 31));
        pic.seq_fields
            .bits
            .set_log2_max_frame_num_minus4(u32::from(sps.log2_max_frame_num_minus4));
        pic.seq_fields
            .bits
            .set_pic_order_cnt_type(u32::from(sps.pic_order_cnt_type));
        pic.seq_fields
            .bits
            .set_log2_max_pic_order_cnt_lsb_minus4(u32::from(
                sps.log2_max_pic_order_cnt_lsb_minus4,
            ));
        pic.seq_fields
            .bits
            .set_delta_pic_order_always_zero_flag(u32::from(sps.delta_pic_order_always_zero_flag));

        pic.pic_fields
            .bits
            .set_entropy_coding_mode_flag(u32::from(pps.entropy_coding_mode_flag));
        pic.pic_fields
            .bits
            .set_weighted_pred_flag(u32::from(pps.weighted_pred_flag));
        pic.pic_fields
            .bits
            .set_weighted_bipred_idc(u32::from(pps.weighted_bipred_idc));
        pic.pic_fields
            .bits
            .set_transform_8x8_mode_flag(u32::from(pps.transform_8x8_mode_flag));
        pic.pic_fields.bits.set_field_pic_flag(0);
        pic.pic_fields
            .bits
            .set_reference_pic_flag(u32::from(nal_ref_idc != 0));
        pic.pic_fields
            .bits
            .set_constrained_intra_pred_flag(u32::from(pps.constrained_intra_pred_flag));
        pic.pic_fields.bits.set_pic_order_present_flag(u32::from(
            pps.bottom_field_pic_order_in_frame_present_flag,
        ));
        pic.pic_fields
            .bits
            .set_deblocking_filter_control_present_flag(u32::from(
                pps.deblocking_filter_control_present_flag,
            ));
        pic.pic_fields
            .bits
            .set_redundant_pic_cnt_present_flag(u32::from(pps.redundant_pic_cnt_present_flag));
    }

    pic
}

fn picture_height_in_mbs_minus1(sps: &H264Sps) -> u16 {
    if sps.frame_mbs_only_flag {
        sps.pic_height_in_map_units_minus1 as u16
    } else {
        sps.pic_height_in_map_units_minus1 as u16 * 2 + 1
    }
}
