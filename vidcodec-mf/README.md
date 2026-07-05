# vidcodec-mf

Windows [Media Foundation](https://learn.microsoft.com/en-us/windows/win32/medfound/media-foundation) backend for [`vidcodec`](../): H.264 hardware encode and decode via MFT transforms.

## Requirements

- Windows 10 or later
- GPU driver with H.264 MFT support (software `CMSH264*MFT` fallbacks are used when hardware MFTs are unavailable)

## Usage

```rust
vidcodec_mf::try_register()?;
let caps = vidcodec::enumerate(vidcodec::Direction::Encode);
```

## Tests

```powershell
cargo test -p vidcodec-mf
```
