//! Links NVENC/NVDEC when built with `--no-default-features` (feature `link`).

fn main() {
    if cfg!(feature = "ci-check") {
        return;
    }

    println!("cargo:rerun-if-env-changed=NVIDIA_VIDEO_CODEC_SDK_PATH");
    println!("cargo:rerun-if-env-changed=NVIDIA_DRIVER_LIB");

    if let Ok(path) = std::env::var("NVIDIA_VIDEO_CODEC_SDK_PATH") {
        println!("cargo:rustc-link-search=native={path}");
    } else if let Ok(path) = std::env::var("NVIDIA_DRIVER_LIB") {
        println!("cargo:rustc-link-search=native={path}");
    }

    // Driver libs (also satisfied when `nvidia-video-codec-sdk` build.rs finds SDK paths).
    println!("cargo:rustc-link-lib=dylib=nvidia-encode");
    println!("cargo:rustc-link-lib=dylib=nvcuvid");
}
