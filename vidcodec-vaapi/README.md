# vidcodec-vaapi

Linux VA-API backend for [vidcodec](../): probes `libva` capabilities and drives
H.264 hardware encode/decode through [`vaapi-sys`](../vaapi-sys).

Bitstream syntax parsing (SPS/PPS/slice headers, Annex-B ↔ AVCC) lives in
[`vidcodec-bitstream`](../vidcodec-bitstream), which wraps [`oxideav-bitstream`](https://crates.io/crates/oxideav-bitstream).

## Usage

```rust
vidcodec_vaapi::try_register()?;

let caps = vidcodec::enumerate(vidcodec::Direction::Encode);
```

## Build

Same as `vaapi-sys` — install `libva-*-dev` packages and `pkg-config`, then:

```bash
cargo build -p vidcodec-vaapi
cargo test -p vidcodec-vaapi
```

Display connection order: **Wayland** (`WAYLAND_DISPLAY`) → **X11** (`DISPLAY`) → **DRM** (`/dev/dri/renderD*`).

## Requirements

- VA driver (`radeonsi`, `iHD`, …) — verify with `vainfo`
- Build: see [vaapi-sys/README.md](../vaapi-sys/README.md)
