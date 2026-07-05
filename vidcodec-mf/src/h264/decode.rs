//! H.264 hardware decoder via Media Foundation.

use bytes::Bytes;
use vidcodec_bitstream::h264::{annex_b_parameter_sets_to_avcc, annex_b_to_length_prefixed};
use vidcodec_core::{
    BitstreamFormat, CodecCapability, DecodedFrame, DecoderConfig, EncodedUnit, Error, PixelFormat,
    VideoDecoder,
};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, IMFTransform, MFT_INPUT_STATUS_ACCEPT_DATA, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STATUS_SAMPLE_READY,
};

use crate::error::{WinResultExt, is_need_more_input};
use crate::media_type::{create_h264_decoder_input_type, create_nv12_output_type};
use crate::mft::{begin_streaming, create_h264_decoder, flush_transform, stream_ids};
use crate::sample::{create_sample_from_bytes, read_nv12_sample};

/// H.264 MF decoder.
pub(crate) struct H264Decoder {
    capability: CodecCapability,
    config: DecoderConfig,
    transform: IMFTransform,
    input_id: u32,
    output_id: u32,
    output_type: Option<IMFMediaType>,
    sequence_header: Option<Vec<u8>>,
    configured: bool,
    extradata_loaded: bool,
}

// SAFETY: H264Decoder only uses IMFTransform from a single thread and contains no !Send fields.
unsafe impl Send for H264Decoder {}

impl H264Decoder {
    /// Opens an H.264 decoder for `cap`.
    pub(crate) fn open(capability: CodecCapability, config: DecoderConfig) -> Result<Self, Error> {
        config.validate()?;
        if config.codec != vidcodec_core::CodecId::H264 {
            return Err(Error::InvalidConfig("H264Decoder requires H.264"));
        }
        if config.output_format != PixelFormat::Nv12 {
            return Err(Error::InvalidConfig("MF H.264 decoder outputs NV12"));
        }
        if !BitstreamFormat::for_codec(config.codec).contains(&config.bitstream) {
            return Err(Error::InvalidConfig("unsupported H.264 bitstream format"));
        }

        let transform = create_h264_decoder()?;
        let (input_id, output_id) = stream_ids(&transform)?;

        let sequence_header = config
            .extradata
            .as_ref()
            .map(|ps| annex_b_parameter_sets_to_avcc(ps))
            .transpose()
            .map_err(|err| Error::backend(err.to_string()))?;

        Ok(Self {
            capability,
            config,
            transform,
            input_id,
            output_id,
            output_type: None,
            sequence_header,
            configured: false,
            extradata_loaded: false,
        })
    }

    fn ensure_configured(&mut self, width: u32, height: u32) -> Result<(), Error> {
        if self.configured {
            return Ok(());
        }

        let input = create_h264_decoder_input_type(width, height, self.sequence_header.as_deref())?;
        let output = create_nv12_output_type(width, height)?;
        // SAFETY: IMFTransform::SetOutputType/SetInputType are safe to call on a valid single-threaded MFT.
        unsafe {
            self.transform
                .SetOutputType(self.output_id, &output, 0)
                .mf()?;
            self.transform.SetInputType(self.input_id, &input, 0).mf()?;
        }
        self.output_type = Some(output);
        begin_streaming(&self.transform)?;
        self.configured = true;
        Ok(())
    }

    fn unit_payload(&self, unit: &EncodedUnit) -> Result<Vec<u8>, Error> {
        match unit.bitstream {
            BitstreamFormat::AnnexB => Ok(annex_b_to_length_prefixed(&unit.data)),
            BitstreamFormat::LengthPrefixed => Ok(unit.data.to_vec()),
            BitstreamFormat::Av1Obu => {
                Err(Error::InvalidBitstream("AV1 bitstream in H.264 decoder"))
            }
        }
    }

    fn decode_payload(
        &mut self,
        payload: &[u8],
        pts: core::time::Duration,
    ) -> Result<Vec<DecodedFrame>, Error> {
        // MF decoder discovers dimensions from the first AU when not pre-configured.
        self.ensure_configured(0, 0)?;

        self.wait_input_ready()?;
        let sample = create_sample_from_bytes(payload, pts)?;
        // SAFETY: IMFTransform::ProcessInput is safe with a valid sample on a single-threaded MFT.
        unsafe {
            self.transform
                .ProcessInput(self.input_id, &sample, 0)
                .mf()?;
        }

        let output_type = self
            .output_type
            .as_ref()
            .ok_or(Error::backend("decoder output type not configured"))?;

        let mut frames = Vec::new();
        for out_sample in self.drain_output()? {
            let (pixels, width, height) = read_nv12_sample(&out_sample, output_type)?;
            frames.push(DecodedFrame {
                pixels: Bytes::from(pixels),
                width,
                height,
                format: self.config.output_format,
                pts,
            });
        }

        if frames.is_empty() {
            return Err(Error::InvalidBitstream("decoder produced no frames"));
        }
        Ok(frames)
    }

    fn wait_input_ready(&self) -> Result<(), Error> {
        for _ in 0..100 {
            // SAFETY: IMFTransform::GetInputStatus returns a dwFlags value; safe with a valid transform.
            let status = unsafe { self.transform.GetInputStatus(self.input_id).mf()? };
            if status & MFT_INPUT_STATUS_ACCEPT_DATA.0 as u32 != 0 {
                return Ok(());
            }
            std::thread::sleep(core::time::Duration::from_millis(1));
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

impl VideoDecoder for H264Decoder {
    fn capability(&self) -> &CodecCapability {
        &self.capability
    }

    fn decode(&mut self, unit: &EncodedUnit) -> Result<Vec<DecodedFrame>, Error> {
        if !self.extradata_loaded {
            if let Some(extradata) = self.config.extradata.clone() {
                let avcc = annex_b_parameter_sets_to_avcc(&extradata)
                    .map_err(|err| Error::backend(err.to_string()))?;
                self.sequence_header = Some(avcc);
            }
            self.extradata_loaded = true;
        }
        if unit.data.is_empty() {
            return Err(Error::InvalidBitstream("empty access unit"));
        }

        let payload = self.unit_payload(unit)?;
        self.decode_payload(&payload, unit.pts)
    }

    fn reset(&mut self) -> Result<(), Error> {
        flush_transform(&self.transform)?;
        self.configured = false;
        self.output_type = None;
        self.extradata_loaded = false;
        Ok(())
    }
}
