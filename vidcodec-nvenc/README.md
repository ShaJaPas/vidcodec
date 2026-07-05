# vidcodec-nvenc

NVIDIA **NVENC** (encode) backend for [vidcodec](../README.md) on **Linux** and **Windows**.

## Requirements

- NVIDIA GPU with NVENC support
- Proprietary NVIDIA driver
- Runtime libraries from the driver:
  - Linux: `libcuda.so.1`, `libnvidia-encode.so.1`
  - Windows: `nvcuda.dll`, `nvEncodeAPI64.dll`

Build uses [`nvidia-video-codec-sdk`](https://crates.io/crates/nvidia-video-codec-sdk).

- **Default (`ci-check`)**: compiles the library without linking NVENC (CI / no GPU).
- **`link`** (`--no-default-features --features link`): links `libnvidia-encode` / `libnvcuvid`.


## Usage

```rust
vidcodec_nvenc::try_register()?;
```

NVDEC decode is not implemented yet; encode supports H.264 (NV12 in, Annex-B or length-prefixed out).
