//! Decoded picture buffer for H.264 VA-API decode.

use vaapi_sys::{
    VA_INVALID_SURFACE, VA_PICTURE_H264_INVALID, VA_PICTURE_H264_SHORT_TERM_REFERENCE,
    VAPictureH264, VASurfaceID,
};
use vidcodec_bitstream::h264::{H264SliceHeader, H264Sps};

/// One decoded reference picture tracked in the DPB.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DpbPicture {
    pub surface: VASurfaceID,
    pub frame_num: u32,
    #[allow(dead_code)]
    pub frame_idx: u32,
    pub top_field_order_cnt: i32,
    pub bottom_field_order_cnt: i32,
}

impl DpbPicture {
    pub(crate) fn to_va_short_term_reference(self) -> VAPictureH264 {
        VAPictureH264 {
            picture_id: self.surface,
            frame_idx: self.frame_num,
            flags: VA_PICTURE_H264_SHORT_TERM_REFERENCE,
            TopFieldOrderCnt: self.top_field_order_cnt,
            BottomFieldOrderCnt: self.bottom_field_order_cnt,
            va_reserved: [0; 4],
        }
    }
}

/// Minimal DPB for low-latency single-reference decode.
#[derive(Debug, Default)]
pub(crate) struct Dpb {
    pictures: Vec<DpbPicture>,
    prev_top_poc: Option<i32>,
}

impl Dpb {
    pub(crate) fn clear(&mut self) {
        self.pictures.clear();
        self.prev_top_poc = None;
    }

    pub(crate) fn references(&self) -> &[DpbPicture] {
        &self.pictures
    }

    pub(crate) fn primary_ref(&self) -> Option<&DpbPicture> {
        self.pictures.last()
    }

    pub(crate) fn prev_top_poc(&self) -> Option<i32> {
        self.prev_top_poc
    }

    pub(crate) fn store(&mut self, picture: DpbPicture, max_refs: usize) {
        self.prev_top_poc = Some(picture.top_field_order_cnt);
        self.pictures.push(picture);
        let cap = max_refs.max(1);
        while self.pictures.len() > cap {
            self.pictures.remove(0);
        }
    }
}

pub(crate) fn field_order_counts(
    sps: &H264Sps,
    slice_header: &H264SliceHeader,
    prev_top_poc: Option<i32>,
) -> (i32, i32) {
    let poc = match sps.pic_order_cnt_type {
        0 => {
            let max_lsb = 1i32 << (sps.log2_max_pic_order_cnt_lsb_minus4 + 4);
            let lsb = slice_header.pic_order_cnt_lsb as i32;
            let msb = match prev_top_poc {
                None => 0,
                Some(prev) => {
                    let prev_lsb = prev.rem_euclid(max_lsb);
                    if lsb < prev_lsb && (prev_lsb - lsb) >= max_lsb / 2 {
                        prev - prev_lsb + max_lsb
                    } else if lsb > prev_lsb && (lsb - prev_lsb) > max_lsb / 2 {
                        prev - prev_lsb - max_lsb
                    } else {
                        prev - prev_lsb
                    }
                }
            };
            msb + lsb
        }
        2 => 2 * slice_header.frame_num as i32,
        _ => 2 * slice_header.frame_num as i32,
    };
    (poc, poc)
}

pub(crate) fn invalid_va_picture() -> VAPictureH264 {
    VAPictureH264 {
        picture_id: VA_INVALID_SURFACE,
        frame_idx: 0,
        flags: VA_PICTURE_H264_INVALID,
        TopFieldOrderCnt: 0,
        BottomFieldOrderCnt: 0,
        va_reserved: [0; 4],
    }
}

pub(crate) fn current_va_picture(
    surface: VASurfaceID,
    frame_num: u32,
    top_poc: i32,
    bottom_poc: i32,
) -> VAPictureH264 {
    VAPictureH264 {
        picture_id: surface,
        frame_idx: frame_num,
        flags: 0,
        TopFieldOrderCnt: top_poc,
        BottomFieldOrderCnt: bottom_poc,
        va_reserved: [0; 4],
    }
}
