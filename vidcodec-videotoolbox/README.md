# vidcodec-videotoolbox

Apple [VideoToolbox](https://developer.apple.com/documentation/videotoolbox) backend for [`vidcodec`](../): H.264 hardware encode and decode on macOS.

## Requirements

- macOS 13.0+
- Apple Silicon or Intel Mac with hardware video codec support

## Usage

```rust
vidcodec_videotoolbox::try_register()?;
let caps = vidcodec::enumerate(vidcodec::Direction::Encode);
```

## Tests

```bash
cargo test -p vidcodec-videotoolbox
```
