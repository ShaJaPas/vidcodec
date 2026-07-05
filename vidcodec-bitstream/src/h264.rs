//! H.264 Annex-B / AVCC helpers built on [`oxideav_bitstream::h264`].

pub use oxideav_bitstream::h264::*;

use vidcodec_core::BitstreamFormat;

use oxideav_bitstream::bit_reader::BitReader;

use crate::BitstreamError;

/// Slice header fields required to populate VA-API decode buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct H264SliceVaInfo {
    /// Minimal slice header fields.
    pub header: H264SliceHeader,
    /// Bit offset from the NAL start to the first slice-data bit.
    pub slice_data_bit_offset: u16,
    /// Parsed `slice_qp_delta`.
    pub slice_qp_delta: i8,
    /// Parsed or defaulted `disable_deblocking_filter_idc`.
    pub disable_deblocking_filter_idc: u8,
    /// Parsed or defaulted `cabac_init_idc`.
    pub cabac_init_idc: u8,
}

/// Parses a slice NAL for VA-API decode buffer population.
///
/// # Errors
///
/// Returns [`BitstreamError`] when the slice header cannot be parsed.
pub fn parse_slice_for_va(
    nal: &[u8],
    sps: &H264Sps,
    pps: &H264Pps,
) -> Result<H264SliceVaInfo, BitstreamError> {
    if nal.is_empty() {
        return Err(BitstreamError::unexpected_end("empty slice NAL"));
    }
    let (_, nal_ref_idc, nal_type) = nal_header(nal[0]);
    let rbsp = ebsp_to_rbsp(&nal[1..]);
    let mut r = BitReader::new(&rbsp);

    let mut header = H264SliceHeader {
        first_mb_in_slice: r.ue()?,
        slice_type: r.ue()? as u8,
        pic_parameter_set_id: r.ue()? as u8,
        ..Default::default()
    };
    if sps.separate_colour_plane_flag {
        r.u(2);
    }
    header.frame_num = r.u(sps.log2_max_frame_num_minus4 as u32 + 4);
    if !sps.frame_mbs_only_flag {
        header.field_pic_flag = r.u(1) != 0;
        if header.field_pic_flag {
            header.bottom_field_flag = r.u(1) != 0;
        }
    }
    if nal_type == NAL_TYPE_IDR {
        header.idr_pic_id = Some(r.ue()?);
    }
    if sps.pic_order_cnt_type == 0 {
        header.pic_order_cnt_lsb = r.u(sps.log2_max_pic_order_cnt_lsb_minus4 as u32 + 4);
        if pps.bottom_field_pic_order_in_frame_present_flag && !header.field_pic_flag {
            let _ = r.se()?;
        }
    } else if sps.pic_order_cnt_type == 1 && !sps.delta_pic_order_always_zero_flag {
        let _ = r.se()?;
    }

    skip_dec_ref_pic_marking(&mut r, nal_type, nal_ref_idc)?;

    if nal_type != 20
        && pps.num_slice_groups_minus1 > 0
        && matches!(header.slice_type % 5, 2 | 0 | 3 | 7 | 5 | 8)
    {
        return Err(BitstreamError::unsupported(
            "slice groups not supported for VA slice parse",
        ));
    }

    if nal_ref_idc != 0 && !matches!(header.slice_type % 5, 2 | 7) {
        skip_ref_pic_list_modification(&mut r)?;
    }
    if matches!(header.slice_type % 5, 1 | 6) {
        skip_ref_pic_list_modification(&mut r)?;
    }

    if (pps.weighted_pred_flag && matches!(header.slice_type % 5, 0 | 5))
        || (pps.weighted_bipred_idc == 1 && matches!(header.slice_type % 5, 1 | 6))
    {
        return Err(BitstreamError::unsupported(
            "weighted prediction not supported for VA slice parse",
        ));
    }

    if nal_type != NAL_TYPE_IDR && nal_type != 20 && nal_ref_idc != 0 {
        skip_dec_ref_pic_marking(&mut r, nal_type, nal_ref_idc)?;
    }

    let cabac_init_idc = if pps.entropy_coding_mode_flag {
        r.ue()? as u8
    } else {
        0
    };
    let slice_qp_delta = r.se()? as i8;
    let disable_deblocking_filter_idc = if pps.deblocking_filter_control_present_flag {
        r.ue()? as u8
    } else {
        0
    };
    if pps.redundant_pic_cnt_present_flag {
        let _ = r.ue()?;
    }

    let offset = 8usize.saturating_add(r.bit_pos());
    let slice_data_bit_offset = u16::try_from(offset)
        .map_err(|_| BitstreamError::invalid("slice_data_bit_offset exceeds u16::MAX"))?;

    Ok(H264SliceVaInfo {
        header,
        slice_data_bit_offset,
        slice_qp_delta,
        disable_deblocking_filter_idc,
        cabac_init_idc,
    })
}

/// Bit offset from the start of a slice NAL (including the NAL header byte)
/// to the first bit of slice data, per VA-API `slice_data_bit_offset`.
///
/// # Errors
///
/// Returns [`BitstreamError`] when the slice header cannot be parsed.
pub fn slice_data_bit_offset(
    nal: &[u8],
    sps: &H264Sps,
    pps: &H264Pps,
) -> Result<u16, BitstreamError> {
    Ok(parse_slice_for_va(nal, sps, pps)?.slice_data_bit_offset)
}

fn skip_dec_ref_pic_marking(
    r: &mut BitReader<'_>,
    nal_type: u8,
    nal_ref_idc: u8,
) -> Result<(), BitstreamError> {
    if nal_type == NAL_TYPE_IDR || nal_type == 20 {
        r.u(1);
        r.u(1);
        return Ok(());
    }
    if nal_ref_idc == 0 {
        return Ok(());
    }
    if r.u(1) == 0 {
        return Ok(());
    }
    loop {
        let op = r.ue()?;
        if op == 0 {
            break;
        }
        match op {
            1 | 3 => {
                let _ = r.ue()?;
            }
            2 => {
                let _ = r.ue()?;
            }
            4..=6 => {}
            other => {
                return Err(BitstreamError::invalid(format!(
                    "unknown memory_management_control_operation {other}"
                )));
            }
        }
    }
    Ok(())
}

fn skip_ref_pic_list_modification(r: &mut BitReader<'_>) -> Result<(), BitstreamError> {
    if r.u(1) == 0 {
        return Ok(());
    }
    loop {
        let idc = r.ue()?;
        if idc == 3 {
            break;
        }
        match idc {
            0 | 1 => {
                let _ = r.ue()?;
            }
            2 => {
                let _ = r.ue()?;
            }
            other => {
                return Err(BitstreamError::invalid(format!(
                    "unknown modification_of_pic_nums_idc {other}"
                )));
            }
        }
    }
    Ok(())
}

/// Collects Annex-B SPS+PPS NAL units from an access unit.
#[must_use]
pub fn collect_parameter_sets_annex_b(annex_b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in split_annex_b(annex_b) {
        if nal.is_empty() {
            continue;
        }
        let (_, _, nal_type) = nal_header(nal[0]);
        if matches!(nal_type, NAL_TYPE_SPS | NAL_TYPE_PPS) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
    }
    out
}

/// Returns `annex_b` with SPS/PPS/AUD NAL units removed, keeping slice data only.
#[must_use]
pub fn strip_parameter_sets_annex_b(annex_b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in split_annex_b(annex_b) {
        if nal.is_empty() {
            continue;
        }
        let (_, _, nal_type) = nal_header(nal[0]);
        if matches!(nal_type, NAL_TYPE_SPS | NAL_TYPE_PPS | 9) {
            continue;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(nal);
    }
    out
}

/// Builds an ISO/IEC 14496-15 `AVCDecoderConfigurationRecord` from Annex-B SPS/PPS.
///
/// # Errors
///
/// Returns [`BitstreamError`] when parameter sets are missing or malformed.
pub fn annex_b_parameter_sets_to_avcc(annex_b_ps: &[u8]) -> Result<Vec<u8>, BitstreamError> {
    let mut sps_list = Vec::new();
    let mut pps_list = Vec::new();
    for nal in split_annex_b(annex_b_ps) {
        if nal.is_empty() {
            continue;
        }
        let (_, _, nal_type) = nal_header(nal[0]);
        match nal_type {
            NAL_TYPE_SPS => sps_list.push(nal.to_vec()),
            NAL_TYPE_PPS => pps_list.push(nal.to_vec()),
            _ => {}
        }
    }
    if sps_list.is_empty() || pps_list.is_empty() {
        return Err(BitstreamError::invalid("missing SPS or PPS for AVCC"));
    }
    let sps = &sps_list[0];
    if sps.len() < 4 {
        return Err(BitstreamError::invalid("SPS too short for AVCC"));
    }

    let mut out = vec![
        1,        // configurationVersion
        sps[1],   // AVCProfileIndication
        sps[2],   // profile_compatibility
        sps[3],   // AVCLevelIndication
        0xFC | 3, // 4-byte NAL length fields
        0xE0 | u8::try_from(sps_list.len())
            .map_err(|_| BitstreamError::invalid("too many SPS NALs for AVCC"))?,
    ];
    for sps in &sps_list {
        let len = u16::try_from(sps.len())
            .map_err(|_| BitstreamError::invalid("SPS NAL too large for AVCC"))?;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(sps);
    }
    out.push(
        u8::try_from(pps_list.len())
            .map_err(|_| BitstreamError::invalid("too many PPS NALs for AVCC"))?,
    );
    for pps in &pps_list {
        let len = u16::try_from(pps.len())
            .map_err(|_| BitstreamError::invalid("PPS NAL too large for AVCC"))?;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(pps);
    }
    Ok(out)
}

/// Converts Annex-B NAL units to 4-byte big-endian length-prefixed form (AVCC).
#[must_use]
pub fn annex_b_to_length_prefixed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for nal in split_annex_b(data) {
        let len = nal.len() as u32;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(nal);
    }
    out
}

/// Converts length-prefixed NALs to Annex-B.
///
/// # Errors
///
/// Returns [`BitstreamError`] when lengths overrun the input.
pub fn length_prefixed_to_annex_b(data: &[u8]) -> Result<Vec<u8>, BitstreamError> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut offset = 0usize;
    while offset + 4 <= data.len() {
        let len = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;
        if offset + len > data.len() {
            return Err(BitstreamError::invalid("length-prefixed NAL overrun"));
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[offset..offset + len]);
        offset += len;
    }
    if offset != data.len() {
        return Err(BitstreamError::invalid("trailing length-prefixed bytes"));
    }
    Ok(out)
}

/// Returns `true` when `annex_b` contains an IDR NAL (type 5).
#[must_use]
pub fn contains_idr(annex_b: &[u8]) -> bool {
    split_annex_b(annex_b)
        .iter()
        .any(|nal| !nal.is_empty() && (nal[0] & 0x1F) == NAL_TYPE_IDR)
}

/// Converts data from a given bitstream format to Annex-B.
///
/// # Errors
///
/// Returns [`BitstreamError::unsupported`] when `source` is [`BitstreamFormat::Av1Obu`]
/// (not valid for H.264).
pub fn to_annex_b(data: &[u8], source: BitstreamFormat) -> Result<Vec<u8>, BitstreamError> {
    match source {
        BitstreamFormat::AnnexB => Ok(data.to_vec()),
        BitstreamFormat::LengthPrefixed => length_prefixed_to_annex_b(data),
        BitstreamFormat::Av1Obu => Err(BitstreamError::unsupported("AV1 OBU in H.264 context")),
    }
}

/// Converts Annex-B data to a target bitstream format.
///
/// # Errors
///
/// Returns [`BitstreamError::unsupported`] when `target` is [`BitstreamFormat::Av1Obu`]
/// (not valid for H.264).
pub fn from_annex_b(annex_b: &[u8], target: BitstreamFormat) -> Result<Vec<u8>, BitstreamError> {
    match target {
        BitstreamFormat::AnnexB => Ok(annex_b.to_vec()),
        BitstreamFormat::LengthPrefixed => Ok(annex_b_to_length_prefixed(annex_b)),
        BitstreamFormat::Av1Obu => Err(BitstreamError::unsupported("AV1 OBU in H.264 context")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avcc_roundtrip() {
        let annex = [0, 0, 0, 1, 0x67, 0x42, 0, 0, 1, 0x68, 0xee];
        let avcc = annex_b_to_length_prefixed(&annex);
        let back = length_prefixed_to_annex_b(&avcc).unwrap();
        assert_eq!(split_annex_b(&back).len(), 2);

        let config = annex_b_parameter_sets_to_avcc(&annex).unwrap();
        assert_eq!(config[0], 1);
    }
}
