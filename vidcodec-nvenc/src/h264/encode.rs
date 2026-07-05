//! H.264 hardware encoder via NVENC.

use core::time::Duration;

use alloc::sync::Arc;

use bytes::Bytes;
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    _NV_ENC_PARAMS_RC_MODE::{NV_ENC_PARAMS_RC_CBR, NV_ENC_PARAMS_RC_VBR},
    NV_ENC_BUFFER_FORMAT, NV_ENC_CONFIG, NV_ENC_PIC_TYPE, NV_ENC_TUNING_INFO,
};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NV_ENC_CODEC_H264_GUID, NV_ENC_H264_PROFILE_BASELINE_GUID, NV_ENC_PRESET_P4_GUID,
};
use nvidia_video_codec_sdk::{EncodePictureParams, Encoder, EncoderInitParams, Session};
use vidcodec_bitstream::h264::{
    annex_b_to_length_prefixed, collect_parameter_sets_annex_b, contains_idr,
};
use vidcodec_core::{
    BitstreamFormat, CodecCapability, EncodedUnit, EncoderConfig, Error, PixelFormat, VideoEncoder,
    VideoFrame,
};

use crate::device::Device;
use crate::error::map_encode;
use crate::profile::profile_to_h264_guid;
use vidcodec_util::pixel::i420_to_nv12;

/// H.264 NVENC encoder session.
pub(crate) struct H264Encoder {
    device: Arc<Device>,
    capability: CodecCapability,
    config: EncoderConfig,
    session: Session,
    force_idr: bool,
    frame_count: u64,
    last_pts: Option<Duration>,
    parameter_sets: Option<Bytes>,
}

// SAFETY: NVENC sessions are tied to one CUDA context; callers must not share across threads.
unsafe impl Send for H264Encoder {}

impl H264Encoder {
    /// Opens an H.264 NVENC encoder.
    ///
    /// # Errors
    ///
    /// Propagates CUDA/NVENC initialization and validation failures.
    pub(crate) fn open(
        device: &Arc<Device>,
        capability: CodecCapability,
        config: EncoderConfig,
    ) -> Result<Self, Error> {
        validate_encoder_config(&config, &capability)?;
        let session = open_session(device, &config)?;
        Ok(Self {
            device: Arc::clone(device),
            capability,
            config,
            session,
            force_idr: true,
            frame_count: 0,
            last_pts: None,
            parameter_sets: None,
        })
    }

    fn rebuild_session(&mut self) -> Result<(), Error> {
        self.session = open_session(&self.device, &self.config)?;
        Ok(())
    }

    fn encode_frame(
        &mut self,
        pixels: &[u8],
        pts: Duration,
        picture_type: NV_ENC_PIC_TYPE,
    ) -> Result<Vec<EncodedUnit>, Error> {
        let frame_bytes = self
            .config
            .input_format
            .frame_size(self.config.width, self.config.height)?;

        if pixels.len() != frame_bytes {
            return Err(Error::PixelBufferMismatch {
                expected: frame_bytes,
                actual: pixels.len(),
            });
        }

        let mut input = self.session.create_input_buffer().map_err(map_encode)?;
        let mut output = self.session.create_output_bitstream().map_err(map_encode)?;

        {
            let mut locked = input.lock().map_err(map_encode)?;
            // SAFETY: `pixels` matches NV12 size for configured width/height.
            unsafe {
                locked.write(pixels);
            }
        }

        let timestamp = pts.as_micros().min(u64::MAX as u128) as u64;
        self.session
            .encode_picture(
                &mut input,
                &mut output,
                EncodePictureParams {
                    input_timestamp: timestamp,
                    picture_type,
                    ..Default::default()
                },
            )
            .map_err(map_encode)?;

        let raw = {
            let locked = output.lock().map_err(map_encode)?;
            locked.data().to_vec()
        };

        let data = match self.config.bitstream {
            BitstreamFormat::AnnexB => raw.clone(),
            BitstreamFormat::LengthPrefixed => annex_b_to_length_prefixed(&raw),
            BitstreamFormat::Av1Obu => {
                return Err(Error::InvalidConfig("AV1 OBU output invalid for H.264"));
            }
        };

        let is_keyframe =
            contains_idr(&raw) || picture_type == NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_IDR;
        if is_keyframe {
            let ps = collect_parameter_sets_annex_b(&raw);
            if !ps.is_empty() {
                self.parameter_sets = Some(Bytes::from(ps));
            }
        }

        self.frame_count += 1;
        self.last_pts = Some(pts);

        Ok(vec![EncodedUnit::new(
            Bytes::from(data),
            is_keyframe,
            pts,
            self.config.bitstream,
        )])
    }
}

fn apply_config(
    encode_config: &mut NV_ENC_CONFIG,
    config: &EncoderConfig,
    profile_guid: nvidia_video_codec_sdk::sys::nvEncodeAPI::GUID,
) {
    encode_config.profileGUID = profile_guid;
    encode_config.gopLength = config.gop_size;
    encode_config.frameIntervalP = 1;

    // SAFETY: preset config from NVENC initializes the codec-config union for H.264.
    unsafe {
        encode_config.encodeCodecConfig.h264Config.idrPeriod = config.gop_size;
        encode_config
            .encodeCodecConfig
            .h264Config
            .set_repeatSPSPPS(1);
        encode_config.encodeCodecConfig.h264Config.set_outputAUD(1);

        if profile_guid == NV_ENC_H264_PROFILE_BASELINE_GUID {
            encode_config
                .encodeCodecConfig
                .h264Config
                .disableDeblockingFilterIDC = 1;
        }
    }

    encode_config.rcParams.rateControlMode = if config.max_bitrate.is_some() {
        NV_ENC_PARAMS_RC_VBR
    } else {
        NV_ENC_PARAMS_RC_CBR
    };
    encode_config.rcParams.averageBitRate = config.bitrate;
    encode_config.rcParams.maxBitRate = config.max_bitrate.unwrap_or(config.bitrate);
}

impl VideoEncoder for H264Encoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn reconfigure(&mut self, config: EncoderConfig) -> Result<(), Error> {
        validate_encoder_config(&config, &self.capability)?;
        self.config = config;
        self.rebuild_session()?;
        self.force_idr = true;
        Ok(())
    }

    fn set_bitrate(&mut self, bitrate_bps: u32) {
        self.config.bitrate = bitrate_bps;
        let _ = self.rebuild_session();
    }

    fn force_keyframe(&mut self) {
        self.force_idr = true;
    }

    fn encode(&mut self, frame: VideoFrame<'_>) -> Result<Vec<EncodedUnit>, Error> {
        frame.validate()?;
        if frame.format != self.config.input_format {
            return Err(Error::InvalidConfig(
                "frame format does not match encoder config",
            ));
        }
        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(Error::InvalidConfig("frame dimensions mismatch"));
        }

        let idr = self.force_idr
            || self.frame_count == 0
            || self.frame_count.is_multiple_of(self.config.gop_size as u64);

        let picture_type = if idr {
            self.force_idr = false;
            NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_IDR
        } else {
            NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_P
        };

        let nv12_buf;
        let pixels = if frame.format == PixelFormat::I420 {
            nv12_buf = i420_to_nv12(frame.pixels, frame.width, frame.height)?;
            nv12_buf.as_slice()
        } else {
            frame.pixels
        };

        self.encode_frame(pixels, frame.pts, picture_type)
    }

    fn last_encoded_pts(&self) -> Option<Duration> {
        self.last_pts
    }

    fn parameter_sets(&self) -> Option<Bytes> {
        self.parameter_sets.clone()
    }
}

fn validate_encoder_config(
    config: &EncoderConfig,
    capability: &CodecCapability,
) -> Result<(), Error> {
    config.validate()?;
    if config.codec != vidcodec_core::CodecId::H264 {
        return Err(Error::InvalidConfig("H264Encoder requires H.264"));
    }
    if config.input_format != PixelFormat::Nv12 && config.input_format != PixelFormat::I420 {
        return Err(Error::InvalidConfig(
            "NVENC H.264 expects NV12 or I420 input",
        ));
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
    Ok(())
}

fn open_session(device: &Arc<Device>, config: &EncoderConfig) -> Result<Session, Error> {
    let profile_guid = profile_to_h264_guid(config.profile)
        .ok_or(Error::InvalidConfig("unsupported H.264 profile"))?;

    let encoder = Encoder::initialize_with_cuda(device.cuda()).map_err(map_encode)?;
    let mut preset = encoder
        .get_preset_config(
            NV_ENC_CODEC_H264_GUID,
            NV_ENC_PRESET_P4_GUID,
            NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
        )
        .map_err(map_encode)?;

    apply_config(&mut preset.presetCfg, config, profile_guid);

    let mut init = EncoderInitParams::new(NV_ENC_CODEC_H264_GUID, config.width, config.height);
    init.preset_guid(NV_ENC_PRESET_P4_GUID)
        .tuning_info(NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY)
        .encode_config(&mut preset.presetCfg)
        .framerate(config.frame_rate.0, config.frame_rate.1)
        .enable_picture_type_decision();

    encoder
        .start_session(NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_NV12, init)
        .map_err(map_encode)
}
