//! Capability probing via NVENC codec/profile GUID queries and NVDEC caps.

use nvidia_video_codec_sdk::Encoder;
use nvidia_video_codec_sdk::sys::cuviddec::{
    CUVIDDECODECAPS, cudaVideoChromaFormat_enum::cudaVideoChromaFormat_420,
    cudaVideoCodec_enum::cudaVideoCodec_H264, cuvidGetDecoderCaps,
};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::NV_ENC_CODEC_H264_GUID;
use vidcodec_core::{
    BackendId, BitstreamFormat, CodecCapability, CodecId, Direction, Error, Profile,
};

use crate::device::Device;
use crate::error::map_encode;
use crate::profile::{codec_to_guid, h264_guid_to_profile, hevc_guid_to_profile};

/// Conservative upper bound when per-codec caps are not queried.
const DEFAULT_MAX_DIMENSION: u32 = 8192;

/// Probes NVENC encode and NVDEC decode capabilities on `device`.
pub(crate) fn probe(device: &Device) -> Result<Vec<CodecCapability>, Error> {
    let mut caps = Vec::new();

    if let Ok(encoder) = Encoder::initialize_with_cuda(device.cuda()) {
        probe_encode(&encoder, &mut caps)?;
    }

    probe_decode(&mut caps)?;
    Ok(caps)
}

fn probe_encode(encoder: &Encoder, caps: &mut Vec<CodecCapability>) -> Result<(), Error> {
    probe_codec(
        encoder,
        CodecId::H264,
        BitstreamFormat::for_codec(CodecId::H264),
        caps,
    )?;
    probe_codec(
        encoder,
        CodecId::Hevc,
        BitstreamFormat::for_codec(CodecId::Hevc),
        caps,
    )?;
    probe_codec(
        encoder,
        CodecId::Av1,
        BitstreamFormat::for_codec(CodecId::Av1),
        caps,
    )?;
    Ok(())
}

fn probe_decode(caps: &mut Vec<CodecCapability>) -> Result<(), Error> {
    if let Some(cap) = probe_h264_decode()? {
        caps.push(cap);
    }
    Ok(())
}

fn probe_h264_decode() -> Result<Option<CodecCapability>, Error> {
    let mut dc = CUVIDDECODECAPS {
        eCodecType: cudaVideoCodec_H264,
        eChromaFormat: cudaVideoChromaFormat_420,
        nBitDepthMinus8: 0,
        reserved1: [0; 3],
        bIsSupported: 0,
        nNumNVDECs: 0,
        nOutputFormatMask: 0,
        nMaxWidth: 0,
        nMaxHeight: 0,
        nMaxMBCount: 0,
        nMinWidth: 0,
        nMinHeight: 0,
        bIsHistogramSupported: 0,
        nCounterBitDepth: 0,
        nMaxHistogramBins: 0,
        reserved3: [0; 10],
    };

    crate::error::map_cuda(
        // SAFETY: `dc` is fully initialized; `cuvidGetDecoderCaps` writes result into it.
        unsafe { cuvidGetDecoderCaps(&mut dc) },
        "cuvidGetDecoderCaps",
    )?;
    if dc.bIsSupported == 0 {
        return Ok(None);
    }

    let max_w = dc.nMaxWidth.max(DEFAULT_MAX_DIMENSION);
    let max_h = dc.nMaxHeight.max(DEFAULT_MAX_DIMENSION);

    Ok(Some(
        CodecCapability::builder(CodecId::H264, BackendId::Nvenc, Direction::Decode)
            .profile(Profile::H264Baseline)
            .profile(Profile::H264Main)
            .profile(Profile::H264High)
            .max_resolution(max_w, max_h)
            .bitstream_formats(vec![
                BitstreamFormat::AnnexB,
                BitstreamFormat::LengthPrefixed,
            ])
            .low_latency(true)
            .build(),
    ))
}

fn probe_codec(
    encoder: &Encoder,
    codec: CodecId,
    bitstream_formats: &[BitstreamFormat],
    caps: &mut Vec<CodecCapability>,
) -> Result<(), Error> {
    let encode_guid = codec_to_guid(codec);
    let encode_guids = encoder.get_encode_guids().map_err(map_encode)?;
    if !encode_guids.contains(&encode_guid) {
        return Ok(());
    }

    let profiles = match codec {
        CodecId::H264 => probe_h264_profiles(encoder)?,
        CodecId::Hevc => probe_hevc_profiles(encoder)?,
        CodecId::Av1 => vec![Profile::Av1Main, Profile::Av1High, Profile::Av1Professional],
    };

    if profiles.is_empty() {
        return Ok(());
    }

    let mut builder = CodecCapability::builder(codec, BackendId::Nvenc, Direction::Encode)
        .max_resolution(DEFAULT_MAX_DIMENSION, DEFAULT_MAX_DIMENSION)
        .bitstream_formats(bitstream_formats.to_vec())
        .low_latency(true);

    for profile in profiles {
        builder = builder.profile(profile);
    }

    caps.push(builder.build());
    Ok(())
}

fn probe_h264_profiles(encoder: &Encoder) -> Result<Vec<Profile>, Error> {
    let guids = encoder
        .get_profile_guids(NV_ENC_CODEC_H264_GUID)
        .map_err(map_encode)?;

    let mut profiles: Vec<Profile> = guids
        .iter()
        .filter_map(|g| h264_guid_to_profile(*g))
        .collect();

    if profiles.is_empty() {
        profiles.push(Profile::H264Main);
    }

    Ok(profiles)
}

fn probe_hevc_profiles(encoder: &Encoder) -> Result<Vec<Profile>, Error> {
    let encode_guid = codec_to_guid(CodecId::Hevc);
    let guids = encoder.get_profile_guids(encode_guid).map_err(map_encode)?;

    let mut profiles: Vec<Profile> = guids
        .iter()
        .filter_map(|g| hevc_guid_to_profile(*g))
        .collect();

    if profiles.is_empty() {
        profiles.push(Profile::HevcMain);
    }

    Ok(profiles)
}
