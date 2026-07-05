//! CUVID parser feeding Annex-B access units.

use core::ffi::{c_ulong, c_void};
use core::ptr;
use core::time::Duration;

use alloc::sync::Arc;
use std::sync::Mutex;

use bytes::Bytes;
use cudarc::driver::CudaContext;
use nvidia_video_codec_sdk::sys::cuviddec::{
    _CUVIDDECODECREATEINFO__bindgen_ty_1, _CUVIDDECODECREATEINFO__bindgen_ty_2,
    CUVIDDECODECREATEINFO, CUVIDPICPARAMS, CUVIDPROCPARAMS, CUvideoctxlock, CUvideodecoder,
    cudaVideoChromaFormat_enum::cudaVideoChromaFormat_420,
    cudaVideoCodec_enum::cudaVideoCodec_H264,
    cudaVideoDeinterlaceMode_enum::cudaVideoDeinterlaceMode_Weave,
    cudaVideoSurfaceFormat_enum::cudaVideoSurfaceFormat_NV12, cuvidCreateDecoder,
    cuvidCtxLockCreate, cuvidCtxLockDestroy, cuvidDecodePicture, cuvidDestroyDecoder,
    cuvidMapVideoFrame64, cuvidUnmapVideoFrame64,
};
use nvidia_video_codec_sdk::sys::nvcuvid::{
    CUVIDEOFORMAT, CUVIDEOFORMAT__bindgen_ty_2, CUVIDPARSERDISPINFO, CUVIDPARSERPARAMS,
    CUVIDSOURCEDATAPACKET,
    CUvideopacketflags::{CUVID_PKT_ENDOFPICTURE, CUVID_PKT_TIMESTAMP},
    CUvideoparser, cuvidCreateVideoParser, cuvidDestroyVideoParser, cuvidParseVideoData,
};
use vidcodec_core::{DecodedFrame, Error, PixelFormat};

use crate::error::map_cuda;
use crate::frame::copy_nv12_from_device;

/// CUVID parser that outputs host NV12 frames.
pub(crate) struct VideoParser {
    parser: CUvideoparser,
    ctx: *mut CallbackCtx,
}

struct CallbackCtx {
    cuda: Arc<CudaContext>,
    ctx_lock: CUvideoctxlock,
    decoder: Mutex<Option<CUvideodecoder>>,
    width: Mutex<u32>,
    height: Mutex<u32>,
    output_format: PixelFormat,
    frames: Mutex<Vec<DecodedFrame>>,
    last_pts: Mutex<Duration>,
}

// SAFETY: Parser callbacks run on the thread that calls `cuvidParseVideoData`.
unsafe impl Send for CallbackCtx {}

impl VideoParser {
    /// Creates an H.264 Annex-B parser.
    pub(crate) fn create_h264(
        cuda: Arc<CudaContext>,
        output_format: PixelFormat,
    ) -> Result<Self, Error> {
        if output_format != PixelFormat::Nv12 {
            return Err(Error::InvalidConfig("NVDEC H.264 outputs NV12"));
        }

        let mut ctx_lock: CUvideoctxlock = ptr::null_mut();
        map_cuda(
            // SAFETY: `cuda.cu_ctx()` returns a valid CUDA context; the lock is created for this context.
            unsafe { cuvidCtxLockCreate(&mut ctx_lock, cuda.cu_ctx()) },
            "cuvidCtxLockCreate",
        )?;

        let ctx = Box::new(CallbackCtx {
            cuda,
            ctx_lock,
            decoder: Mutex::new(None),
            width: Mutex::new(0),
            height: Mutex::new(0),
            output_format,
            frames: Mutex::new(Vec::new()),
            last_pts: Mutex::new(Duration::ZERO),
        });
        let ctx_ptr = Box::into_raw(ctx);

        let mut parser: CUvideoparser = ptr::null_mut();
        let mut params = CUVIDPARSERPARAMS {
            CodecType: cudaVideoCodec_H264,
            ulMaxNumDecodeSurfaces: 4,
            ulClockRate: 1_000_000,
            ulErrorThreshold: 0,
            ulMaxDisplayDelay: 0,
            _bitfield_align_1: [0u32; 0],
            _bitfield_1: CUVIDPARSERPARAMS::new_bitfield_1(1, 0),
            uReserved1: [0; 4],
            pUserData: ctx_ptr.cast(),
            pfnSequenceCallback: Some(handle_video_sequence),
            pfnDecodePicture: Some(handle_picture_decode),
            pfnDisplayPicture: Some(handle_picture_display),
            pfnGetOperatingPoint: None,
            pfnGetSEIMsg: None,
            pvReserved2: [ptr::null_mut(); 5],
            pExtVideoInfo: ptr::null_mut(),
        };

        map_cuda(
            // SAFETY: `parser` is uninitialized and `params` is fully initialized; NVCUVID API is called with valid FFI params.
            unsafe { cuvidCreateVideoParser(&mut parser, &mut params) },
            "cuvidCreateVideoParser",
        )?;

        Ok(Self {
            parser,
            ctx: ctx_ptr,
        })
    }

    /// Feeds one Annex-B access unit and returns decoded frames (usually one).
    pub(crate) fn feed(
        &mut self,
        annex_b: &[u8],
        pts: Duration,
    ) -> Result<Vec<DecodedFrame>, Error> {
        // SAFETY: `self.ctx` is a valid `Box<CallbackCtx>` pointer produced by `Box::into_raw`.
        let ctx = unsafe { &*self.ctx };
        ctx.frames.lock().expect("frame mutex").clear();
        *ctx.last_pts.lock().expect("pts mutex") = pts;

        let mut packet = CUVIDSOURCEDATAPACKET {
            flags: CUVID_PKT_ENDOFPICTURE as c_ulong | CUVID_PKT_TIMESTAMP as c_ulong,
            payload_size: annex_b.len() as c_ulong,
            payload: annex_b.as_ptr(),
            timestamp: pts.as_micros().min(i64::MAX as u128) as i64,
        };

        map_cuda(
            // SAFETY: `self.parser` is a valid `CUvideoparser` handle created by `cuvidCreateVideoParser`.
            unsafe { cuvidParseVideoData(self.parser, &mut packet) },
            "cuvidParseVideoData",
        )?;

        Ok(ctx.frames.lock().expect("frame mutex").drain(..).collect())
    }

    /// Tears down decoder state while keeping the parser object.
    pub(crate) fn reset_decoder(&mut self) -> Result<(), Error> {
        // SAFETY: `self.ctx` is a valid `Box<CallbackCtx>` pointer produced by `Box::into_raw`.
        let ctx = unsafe { &*self.ctx };
        if let Some(decoder) = ctx.decoder.lock().expect("decoder mutex").take() {
            map_cuda(
                // SAFETY: `decoder` is a valid `CUvideodecoder` handle created by `cuvidCreateDecoder`.
                unsafe { cuvidDestroyDecoder(decoder) },
                "cuvidDestroyDecoder",
            )?;
        }
        *ctx.width.lock().expect("width mutex") = 0;
        *ctx.height.lock().expect("height mutex") = 0;
        Ok(())
    }
}

impl Drop for VideoParser {
    fn drop(&mut self) {
        if !self.parser.is_null() {
            // SAFETY: `self.parser` is a valid `CUvideoparser` handle created by `cuvidCreateVideoParser`.
            unsafe {
                let _ = cuvidDestroyVideoParser(self.parser);
            }
        }
        if !self.ctx.is_null() {
            // SAFETY: `self.ctx` is the raw pointer from `Box::into_raw` and is only reclaimed once.
            let ctx = unsafe { Box::from_raw(self.ctx) };
            if let Some(decoder) = ctx.decoder.lock().expect("decoder mutex").take() {
                // SAFETY: `decoder` is a valid `CUvideodecoder` handle created by `cuvidCreateDecoder`.
                unsafe {
                    let _ = cuvidDestroyDecoder(decoder);
                }
            }
            if !ctx.ctx_lock.is_null() {
                // SAFETY: `ctx.ctx_lock` is a valid lock created by `cuvidCtxLockCreate`.
                unsafe {
                    let _ = cuvidCtxLockDestroy(ctx.ctx_lock);
                }
            }
        }
    }
}

unsafe extern "C" fn handle_video_sequence(
    user_data: *mut c_void,
    format: *mut CUVIDEOFORMAT,
) -> i32 {
    // SAFETY: `user_data` is `ctx_ptr` cast to `*mut c_void` — always a valid `CallbackCtx`.
    let ctx = unsafe { &mut *(user_data.cast::<CallbackCtx>()) };
    // SAFETY: `format` is a valid pointer provided by the NVCUVID parser callback.
    let format = unsafe { &*format };

    if let Some(decoder) = ctx.decoder.lock().expect("decoder mutex").take() {
        // SAFETY: `decoder` is a valid `CUvideodecoder` handle created earlier.
        unsafe {
            let _ = cuvidDestroyDecoder(decoder);
        }
    }

    let width = format.coded_width;
    let height = format.coded_height;
    let display_w = (format.display_area.right - format.display_area.left).max(0) as u32;
    let display_h = (format.display_area.bottom - format.display_area.top).max(0) as u32;
    let out_w = if display_w > 0 { display_w } else { width };
    let out_h = if display_h > 0 { display_h } else { height };

    *ctx.width.lock().expect("width mutex") = out_w;
    *ctx.height.lock().expect("height mutex") = out_h;

    let display_area = to_decode_display_area(&format.display_area);
    let target_rect = to_decode_target_rect(&format.display_area);

    let mut info = CUVIDDECODECREATEINFO {
        ulWidth: width as c_ulong,
        ulHeight: height as c_ulong,
        ulNumDecodeSurfaces: format.min_num_decode_surfaces.max(4) as c_ulong,
        CodecType: cudaVideoCodec_H264,
        ChromaFormat: cudaVideoChromaFormat_420,
        ulCreationFlags: 0,
        bitDepthMinus8: 0,
        ulIntraDecodeOnly: 0,
        ulMaxWidth: width as c_ulong,
        ulMaxHeight: height as c_ulong,
        Reserved1: 0,
        display_area,
        OutputFormat: cudaVideoSurfaceFormat_NV12,
        DeinterlaceMode: cudaVideoDeinterlaceMode_Weave,
        ulTargetWidth: out_w as c_ulong,
        ulTargetHeight: out_h as c_ulong,
        ulNumOutputSurfaces: 2,
        vidLock: ctx.ctx_lock,
        target_rect,
        enableHistogram: 0,
        Reserved2: [0; 4],
    };

    let mut decoder: CUvideodecoder = ptr::null_mut();
    if map_cuda(
        // SAFETY: `decoder` is uninitialized and `info` is fully initialized; valid NVCUVID FFI call.
        unsafe { cuvidCreateDecoder(&mut decoder, &mut info) },
        "cuvidCreateDecoder",
    )
    .is_err()
    {
        return 0;
    }

    *ctx.decoder.lock().expect("decoder mutex") = Some(decoder);
    format.min_num_decode_surfaces.max(1) as i32
}

unsafe extern "C" fn handle_picture_decode(
    user_data: *mut c_void,
    pic_params: *mut CUVIDPICPARAMS,
) -> i32 {
    // SAFETY: `user_data` is `ctx_ptr` cast — always a valid `CallbackCtx`.
    let ctx = unsafe { &*(user_data.cast::<CallbackCtx>()) };
    let Some(decoder) = *ctx.decoder.lock().expect("decoder mutex") else {
        return 0;
    };

    if map_cuda(
        // SAFETY: `decoder` is a valid `CUvideodecoder` and `pic_params` is provided by the NVCUVID parser.
        unsafe { cuvidDecodePicture(decoder, pic_params) },
        "cuvidDecodePicture",
    )
    .is_err()
    {
        return 0;
    }
    1
}

unsafe extern "C" fn handle_picture_display(
    user_data: *mut c_void,
    disp_info: *mut CUVIDPARSERDISPINFO,
) -> i32 {
    // SAFETY: `user_data` is `ctx_ptr` cast — always a valid `CallbackCtx`.
    let ctx = unsafe { &*(user_data.cast::<CallbackCtx>()) };
    // SAFETY: `disp_info` is a valid pointer provided by the NVCUVID parser callback.
    let disp_info = unsafe { &*disp_info };
    let Some(decoder) = *ctx.decoder.lock().expect("decoder mutex") else {
        return 0;
    };

    let width = *ctx.width.lock().expect("width mutex");
    let height = *ctx.height.lock().expect("height mutex");
    if width == 0 || height == 0 {
        return 0;
    }

    let mut dev_ptr: u64 = 0;
    let mut pitch: u32 = 0;
    let mut proc = CUVIDPROCPARAMS {
        progressive_frame: disp_info.progressive_frame,
        second_field: 0,
        top_field_first: disp_info.top_field_first,
        unpaired_field: 0,
        reserved_flags: 0,
        reserved_zero: 0,
        raw_input_dptr: 0,
        raw_input_pitch: 0,
        raw_input_format: 0,
        raw_output_dptr: 0,
        raw_output_pitch: 0,
        Reserved1: 0,
        output_stream: ptr::null_mut(),
        Reserved: [0; 46],
        histogram_dptr: ptr::null_mut(),
        Reserved2: [ptr::null_mut(); 1],
    };

    if map_cuda(
        // SAFETY: `decoder` is a valid decoder handle; `dev_ptr` and `pitch` are output params.
        unsafe {
            cuvidMapVideoFrame64(
                decoder,
                disp_info.picture_index,
                &mut dev_ptr,
                &mut pitch,
                &mut proc,
            )
        },
        "cuvidMapVideoFrame64",
    )
    .is_err()
    {
        return 0;
    }

    let pixels = match copy_nv12_from_device(&ctx.cuda, dev_ptr, pitch, width, height) {
        Ok(pixels) => pixels,
        Err(_) => {
            // SAFETY: `decoder` is valid and `dev_ptr` was mapped by `cuvidMapVideoFrame64` above.
            unsafe {
                let _ = cuvidUnmapVideoFrame64(decoder, dev_ptr);
            }
            return 0;
        }
    };

    // SAFETY: `decoder` is valid and `dev_ptr` was mapped by `cuvidMapVideoFrame64` above.
    unsafe {
        let _ = cuvidUnmapVideoFrame64(decoder, dev_ptr);
    }

    let pts = *ctx.last_pts.lock().expect("pts mutex");
    ctx.frames.lock().expect("frame mutex").push(DecodedFrame {
        pixels: Bytes::from(pixels),
        width,
        height,
        format: ctx.output_format,
        pts,
    });

    1
}

fn to_decode_display_area(
    area: &CUVIDEOFORMAT__bindgen_ty_2,
) -> _CUVIDDECODECREATEINFO__bindgen_ty_1 {
    _CUVIDDECODECREATEINFO__bindgen_ty_1 {
        left: area.left as i16,
        top: area.top as i16,
        right: area.right as i16,
        bottom: area.bottom as i16,
    }
}

fn to_decode_target_rect(
    area: &CUVIDEOFORMAT__bindgen_ty_2,
) -> _CUVIDDECODECREATEINFO__bindgen_ty_2 {
    _CUVIDDECODECREATEINFO__bindgen_ty_2 {
        left: area.left as i16,
        top: area.top as i16,
        right: area.right as i16,
        bottom: area.bottom as i16,
    }
}
