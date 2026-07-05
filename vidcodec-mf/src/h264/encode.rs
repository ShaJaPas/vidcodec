//! H.264 hardware encoder via Media Foundation.

use core::time::Duration;

use bytes::Bytes;
use vidcodec_bitstream::h264::{
    annex_b_to_length_prefixed, collect_parameter_sets_annex_b, contains_idr,
    length_prefixed_to_annex_b,
};
use vidcodec_core::{
    BitstreamFormat, CodecCapability, EncodedUnit, EncoderConfig, Error, PixelFormat, VideoEncoder,
    VideoFrame,
};
use windows::Win32::Media::MediaFoundation::{
    CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncMPVGOPSize, CODECAPI_AVEncVideoForceKeyFrame,
    ICodecAPI, IMFTransform, MFT_INPUT_STATUS_ACCEPT_DATA, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STATUS_SAMPLE_READY,
};
use windows::Win32::System::Variant::{VARIANT, VT_UI4};
use windows::core::Interface;

use crate::error::{WinResultExt, is_need_more_input};
use crate::media_type::{create_h264_output_type, create_nv12_input_type};
use crate::mft::{begin_streaming, create_h264_encoder, flush_transform, stream_ids};
use crate::sample::{create_nv12_sample, read_sample_bytes};
use vidcodec_util::pixel::i420_to_nv12;

/// H.264 MF encoder session.
pub(crate) struct H264Encoder {
    capability: CodecCapability,
    config: EncoderConfig,
    transform: IMFTransform,
    codec_api: Option<ICodecAPI>,
    input_id: u32,
    output_id: u32,
    force_idr: bool,
    frame_count: u64,
    last_pts: Option<Duration>,
    parameter_sets: Option<Bytes>,
}

// MF encoder sessions are not thread-safe.
// SAFETY: H264Encoder only uses IMFTransform from a single thread and contains no !Send fields.
unsafe impl Send for H264Encoder {}

impl H264Encoder {
    /// Opens an H.264 encoder for `cap` using `config`.
    pub(crate) fn open(capability: CodecCapability, config: EncoderConfig) -> Result<Self, Error> {
        validate_encoder_config(&config, &capability)?;

        let transform = create_h264_encoder()?;
        let (input_id, output_id) = stream_ids(&transform)?;
        let codec_api = transform.cast().ok();

        let mut enc = Self {
            capability,
            config,
            transform,
            codec_api,
            input_id,
            output_id,
            force_idr: true,
            frame_count: 0,
            last_pts: None,
            parameter_sets: None,
        };
        enc.apply_types()?;
        enc.apply_codec_properties()?;
        begin_streaming(&enc.transform)?;
        Ok(enc)
    }

    fn apply_types(&mut self) -> Result<(), Error> {
        let output = create_h264_output_type(&self.config)?;
        let input = create_nv12_input_type(&self.config)?;
        // SAFETY: IMFTransform::SetOutputType/SetInputType are safe on a valid single-threaded MFT.
        unsafe {
            self.transform
                .SetOutputType(self.output_id, &output, 0)
                .mf()?;
            self.transform.SetInputType(self.input_id, &input, 0).mf()?;
        }
        Ok(())
    }

    fn apply_codec_properties(&self) -> Result<(), Error> {
        let Some(codec_api) = &self.codec_api else {
            return Ok(());
        };
        set_codecapi_u32(codec_api, &CODECAPI_AVEncMPVGOPSize, self.config.gop_size)?;
        set_codecapi_u32(
            codec_api,
            &CODECAPI_AVEncCommonMeanBitRate,
            self.config.bitrate,
        )?;
        Ok(())
    }

    fn rebuild(&mut self) -> Result<(), Error> {
        flush_transform(&self.transform)?;
        self.apply_types()?;
        self.apply_codec_properties()?;
        begin_streaming(&self.transform)?;
        Ok(())
    }

    fn encode_nv12(
        &mut self,
        pixels: &[u8],
        pts: Duration,
        force_idr: bool,
    ) -> Result<Vec<EncodedUnit>, Error> {
        if force_idr {
            if let Some(codec_api) = &self.codec_api {
                set_codecapi_u32(codec_api, &CODECAPI_AVEncVideoForceKeyFrame, 1)?;
            }
        }

        self.wait_input_ready()?;
        let sample = create_nv12_sample(pixels, pts)?;
        // SAFETY: IMFTransform::ProcessInput is safe with a valid sample on a single-threaded MFT.
        unsafe {
            self.transform
                .ProcessInput(self.input_id, &sample, 0)
                .mf()?;
        }

        let mut units = Vec::new();
        for out_sample in self.drain_output()? {
            let raw = read_sample_bytes(&out_sample)?;
            let annex_b =
                length_prefixed_to_annex_b(&raw).map_err(|err| Error::backend(err.to_string()))?;

            let data = match self.config.bitstream {
                BitstreamFormat::AnnexB => annex_b.clone(),
                BitstreamFormat::LengthPrefixed => annex_b_to_length_prefixed(&annex_b),
                BitstreamFormat::Av1Obu => {
                    return Err(Error::InvalidConfig("AV1 OBU output invalid for H.264"));
                }
            };

            let is_keyframe = contains_idr(&annex_b) || force_idr;
            if is_keyframe {
                let ps = collect_parameter_sets_annex_b(&annex_b);
                if !ps.is_empty() {
                    self.parameter_sets = Some(Bytes::from(ps));
                }
            }

            self.frame_count += 1;
            self.last_pts = Some(pts);

            units.push(EncodedUnit::new(
                Bytes::from(data),
                is_keyframe,
                pts,
                self.config.bitstream,
            ));
        }

        if units.is_empty() {
            return Err(Error::backend("encoder produced no output samples"));
        }
        Ok(units)
    }

    fn wait_input_ready(&self) -> Result<(), Error> {
        for _ in 0..100 {
            // SAFETY: IMFTransform::GetInputStatus returns a dwFlags value; safe with a valid transform.
            let status = unsafe { self.transform.GetInputStatus(self.input_id).mf()? };
            if status & MFT_INPUT_STATUS_ACCEPT_DATA.0 as u32 != 0 {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(Error::Backpressure)
    }

    fn drain_output(
        &self,
    ) -> Result<Vec<windows::Win32::Media::MediaFoundation::IMFSample>, Error> {
        let mut samples = Vec::new();
        loop {
            // SAFETY: IMFTransform::GetOutputStatus returns a dwFlags value; safe with a valid transform.
            let status = unsafe { self.transform.GetOutputStatus().mf()? };
            if status & MFT_OUTPUT_STATUS_SAMPLE_READY.0 as u32 == 0 {
                break;
            }

            let mut out_buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: self.output_id,
                pSample: core::mem::ManuallyDrop::new(None),
                dwStatus: 0,
                pEvents: core::mem::ManuallyDrop::new(None),
            }];
            let mut proc_status = 0u32;
            // SAFETY: IMFTransform::ProcessOutput is safe on a single-threaded MFT with caller-managed buffers.
            let hr = unsafe {
                self.transform
                    .ProcessOutput(0, &mut out_buffers, &mut proc_status)
            };
            match hr {
                Ok(()) => {}
                Err(err) if is_need_more_input(&err) => break,
                Err(err) => return Err(Error::backend(format!("ProcessOutput: {err}"))),
            }

            // SAFETY: ManuallyDrop::take is safe because out_buffers[0].pSample is initialized and not reused.
            let sample = unsafe { core::mem::ManuallyDrop::take(&mut out_buffers[0].pSample) };
            if let Some(sample) = sample {
                samples.push(sample);
            } else {
                break;
            }
        }
        Ok(samples)
    }
}

impl VideoEncoder for H264Encoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn reconfigure(&mut self, config: EncoderConfig) -> Result<(), Error> {
        validate_encoder_config(&config, &self.capability)?;
        self.config = config;
        self.rebuild()?;
        self.force_idr = true;
        self.frame_count = 0;
        self.parameter_sets = None;
        Ok(())
    }

    fn set_bitrate(&mut self, bitrate_bps: u32) {
        self.config.bitrate = bitrate_bps;
        if let Some(codec_api) = &self.codec_api {
            let _ = set_codecapi_u32(codec_api, &CODECAPI_AVEncCommonMeanBitRate, bitrate_bps);
        } else {
            let _ = self.rebuild();
        }
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
        self.force_idr = false;

        let nv12_buf;
        let pixels = if frame.format == PixelFormat::I420 {
            nv12_buf = i420_to_nv12(frame.pixels, frame.width, frame.height)?;
            nv12_buf.as_slice()
        } else {
            frame.pixels
        };

        self.encode_nv12(pixels, frame.pts, idr)
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
        return Err(Error::InvalidConfig("MF H.264 expects NV12 or I420 input"));
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

fn set_codecapi_u32(
    codec_api: &ICodecAPI,
    key: &windows::core::GUID,
    value: u32,
) -> Result<(), Error> {
    // SAFETY: VARIANT is zeroed and then vt + ulVal are written at non-overlapping offsets.
    let variant = unsafe {
        let mut v: VARIANT = core::mem::zeroed();
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = value;
        v
    };
    // SAFETY: ICodecAPI::SetValue is safe on a single-threaded codec API instance.
    unsafe {
        codec_api.SetValue(key, &variant).mf()?;
    }
    Ok(())
}
