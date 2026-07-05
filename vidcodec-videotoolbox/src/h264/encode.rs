//! H.264 hardware encoder via VideoToolbox.

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
use videotoolbox::compression::CompressionSession;
use videotoolbox::session::Codec;

use crate::encode_opts::encode_forced_keyframe;
use crate::error::map_vt;
use vidcodec_util::pixel::i420_to_nv12;

use crate::pixel::nv12_pixel_buffer_from_host;
use crate::profile::profile_to_vt;

/// H.264 VideoToolbox encoder session.
pub(crate) struct H264Encoder {
    capability: CodecCapability,
    config: EncoderConfig,
    session: CompressionSession,
    force_idr: bool,
    frame_count: u64,
    last_pts: Option<Duration>,
    parameter_sets: Option<Bytes>,
}

// SAFETY: H264Encoder owns only Send types (CompressionSession, Bytes, etc.).
unsafe impl Send for H264Encoder {}

impl H264Encoder {
    /// Opens an H.264 encoder for `cap` using `config`.
    pub(crate) fn open(capability: CodecCapability, config: EncoderConfig) -> Result<Self, Error> {
        validate_encoder_config(&config, &capability)?;
        let session = open_session(&config)?;
        Ok(Self {
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
        self.session = open_session(&self.config)?;
        Ok(())
    }

    fn presentation_time(&self, pts: Duration) -> (i64, i32) {
        let micros = pts.as_micros().min(i64::MAX as u128) as i64;
        (micros, 1_000_000)
    }
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
        self.frame_count = 0;
        self.parameter_sets = None;
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
        self.force_idr = false;

        let nv12_buf;
        let pixels = if frame.format == PixelFormat::I420 {
            nv12_buf = i420_to_nv12(frame.pixels, frame.width, frame.height)?;
            nv12_buf.as_slice()
        } else {
            frame.pixels
        };

        let pixel_buffer = nv12_pixel_buffer_from_host(pixels, frame.width, frame.height)?;

        let raw_data = if idr {
            encode_forced_keyframe(
                &self.session,
                pixel_buffer,
                self.presentation_time(frame.pts),
            )?
        } else {
            let surface = pixel_buffer
                .io_surface()
                .ok_or_else(|| Error::backend("NV12 pixel buffer is not IOSurface-backed"))?;
            self.session
                .encode(&surface, self.presentation_time(frame.pts))
                .map_err(map_vt)?
                .data
        };

        let annex_b =
            length_prefixed_to_annex_b(&raw_data).map_err(|err| Error::backend(err.to_string()))?;
        let data = match self.config.bitstream {
            BitstreamFormat::AnnexB => annex_b.clone(),
            BitstreamFormat::LengthPrefixed => annex_b_to_length_prefixed(&annex_b),
            BitstreamFormat::Av1Obu => {
                return Err(Error::InvalidConfig("AV1 OBU output invalid for H.264"));
            }
        };

        let is_keyframe = contains_idr(&annex_b) || idr;
        if is_keyframe {
            let ps = collect_parameter_sets_annex_b(&annex_b);
            if !ps.is_empty() {
                self.parameter_sets = Some(Bytes::from(ps));
            }
        }

        self.frame_count += 1;
        self.last_pts = Some(frame.pts);

        Ok(vec![EncodedUnit::new(
            Bytes::from(data),
            is_keyframe,
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
            "VideoToolbox H.264 expects NV12 or I420 input",
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
    profile_to_vt(config.profile).ok_or(Error::InvalidConfig("unsupported H.264 profile"))?;
    Ok(())
}

fn open_session(config: &EncoderConfig) -> Result<CompressionSession, Error> {
    let fps = config.frame_rate.0 as f64 / config.frame_rate.1 as f64;
    let mut builder = CompressionSession::builder(
        i32::try_from(config.width).map_err(|_| Error::InvalidConfig("width too large"))?,
        i32::try_from(config.height).map_err(|_| Error::InvalidConfig("height too large"))?,
        Codec::H264,
    )
    .with_real_time(true)
    .with_allow_frame_reordering(false)
    .with_average_bit_rate(
        i32::try_from(config.bitrate).map_err(|_| Error::InvalidConfig("bitrate too large"))?,
    )
    .with_expected_frame_rate(fps)
    .with_max_keyframe_interval(
        i32::try_from(config.gop_size).map_err(|_| Error::InvalidConfig("gop_size too large"))?,
    );

    if let Some(profile) = profile_to_vt(config.profile) {
        builder = builder.with_profile_level(profile);
    }

    builder.build().map_err(map_vt)
}
