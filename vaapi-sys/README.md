# vaapi-sys

Low-level Rust FFI to [VA-API](https://github.com/intel/libva) (`libva`).

Part of the [vidcodec](../) workspace. Bindings are generated at **build time** via `bindgen` from installed libva headers (`clang` required).

## How linking works (standard Linux)

Like GStreamer, FFmpeg, or any other native multimedia crate:

1. **Build time** — `pkg-config` finds `libva` and emits linker flags. No path guessing.
2. **Runtime** — the system's `libva` loads the GPU driver from standard locations (`/usr/lib/dri`, …) or from `LIBVA_DRIVERS_PATH` if the distro sets it.

Vidcodec does **not** embed distro-specific paths or call `nix eval` at runtime.

### Build dependencies

| Distro | Packages |
|--------|----------|
| Debian / Ubuntu | `libva-dev` `libva-drm-dev` `libva-wayland-dev` `libva-x11-dev` `pkg-config` `clang` |
| Fedora | `libva-devel` `libva-drm-devel` `libva-wayland-devel` `libva-x11-devel` `pkg-config` `clang` |
| Arch | `libva` `clang` (+ Mesa VA driver for your GPU) |

```bash
cargo build -p vaapi-sys
```

**Runtime:** a VA driver for your GPU (`mesa` / `radeonsi` for AMD, `intel-media-driver` for Intel). On FHS distros this is installed with Mesa/Intel packages and just works.

### NixOS

NixOS is not FHS — libraries and drivers live in `/nix/store`, so plain `cargo build` outside a dev shell will not find libva via `pkg-config`.

Use the repo dev shell (sets `pkg-config`, `LIBVA_DRIVERS_PATH`, etc.):

```bash
nix-shell shell.nix
cargo run --example enumerate -p vidcodec
```

For packaged Nix apps, wrap the binary with `makeWrapper` and set `LIBVA_DRIVERS_PATH` — same as `firefox`, `vlc`, etc.

Override for exotic setups: `LIBVA_LIB_DIR=/path/to/libva/lib cargo build`.

## Features

| Feature | Default | Links / headers |
|---------|---------|-----------------|
| `drm` | yes | `libva-drm`, `va/va_drm.h` |
| `wayland` | yes | `libva-wayland`, `va/va_wayland.h` |
| `x11` | yes | `libva-x11`, `va/va_x11.h` |

## License

Apache-2.0
