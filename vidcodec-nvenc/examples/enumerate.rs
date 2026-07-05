//! Lists NVENC capabilities (requires NVIDIA GPU + driver).
//!
//! ```bash
//! cargo run --example enumerate -p vidcodec-nvenc
//! ```

use vidcodec::{Direction, enumerate};

fn main() {
    match vidcodec_nvenc::try_register() {
        Ok(()) => eprintln!("registered NVENC backend"),
        Err(err) => {
            eprintln!("NVENC unavailable: {err}");
            std::process::exit(1);
        }
    }

    for direction in [Direction::Encode, Direction::Decode] {
        let caps = enumerate(direction);
        println!("{direction:?} ({} capabilities)", caps.len());
        for cap in caps {
            let profiles: Vec<_> = cap.profiles.iter().map(|p| p.wire_name()).collect();
            println!(
                "  {} via {} — profiles [{}], max {}×{}",
                cap.codec.wire_name(),
                cap.backend.name(),
                profiles.join(", "),
                cap.max_width,
                cap.max_height,
            );
        }
        println!();
    }
}
