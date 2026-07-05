//! Lists hardware encode/decode capabilities from enabled backend crates.
//!
//! ```bash
//! cargo run -p vidcodec-probe                         # platform default backends
//! cargo run -p vidcodec-probe --features nvenc        # + NVENC (needs NVIDIA libs)
//! ```

use vidcodec::{Direction, enumerate};

fn main() {
    for direction in [Direction::Encode, Direction::Decode] {
        let caps = enumerate(direction);
        println!("{direction:?} ({} capabilities)", caps.len());

        if caps.is_empty() {
            println!("  (none — no backend registered or no hardware found)");
            println!();
            continue;
        }

        for cap in caps {
            let profiles: Vec<_> = cap.profiles.iter().map(|p| p.wire_name()).collect();
            let bitstreams: Vec<_> = cap
                .bitstream_formats
                .iter()
                .map(|b| format!("{b:?}"))
                .collect();

            println!(
                "  {} via {} — profiles [{}], max {}×{}, bitstream [{}], low_latency={}",
                cap.codec.wire_name(),
                cap.backend.name(),
                profiles.join(", "),
                cap.max_width,
                cap.max_height,
                bitstreams.join(", "),
                cap.low_latency,
            );
        }
        println!();
    }
}
