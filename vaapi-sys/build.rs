use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=LIBVA_LIB_DIR");

    link_libraries();
    generate_bindings();
}

fn link_libraries() {
    let libs = required_libs();

    if pkg_config_links_all(&libs) {
        return;
    }

    let dir = match env::var("LIBVA_LIB_DIR").ok().filter(|dir| !dir.is_empty()) {
        Some(dir) => dir,
        None => panic_libva_not_found(),
    };

    if !libva_present(&dir) {
        panic!(
            "LIBVA_LIB_DIR={dir} does not contain libva (expected libva.so); \
             install libva development packages or point LIBVA_LIB_DIR at the directory that does"
        );
    }

    println!("cargo:rustc-link-search=native={dir}");
    for (_, lib) in libs {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}

fn required_libs() -> Vec<(&'static str, &'static str)> {
    let mut libs = vec![("libva", "va")];
    if env::var("CARGO_FEATURE_DRM").is_ok() {
        libs.push(("libva-drm", "va-drm"));
    }
    if env::var("CARGO_FEATURE_X11").is_ok() {
        libs.push(("libva-x11", "va-x11"));
    }
    if env::var("CARGO_FEATURE_WAYLAND").is_ok() {
        libs.push(("libva-wayland", "va-wayland"));
    }
    libs
}

fn pkg_config_links_all(libs: &[(&str, &str)]) -> bool {
    libs.iter().all(|(pkg, _)| {
        pkg_config::Config::new()
            .probe(pkg)
            .inspect_err(|err| {
                eprintln!("cargo:warning=pkg-config probe for {pkg} failed: {err}");
            })
            .is_ok()
    })
}

fn libva_present(dir: &str) -> bool {
    ["libva.so", "libva.so.2", "libva.so.0"]
        .into_iter()
        .any(|name| std::path::Path::new(dir).join(name).exists())
}

fn panic_libva_not_found() -> ! {
    panic!(
        "libva not found via pkg-config.\n\
         \n\
         Debian/Ubuntu: sudo apt install libva-dev libva-drm-dev libva-wayland-dev libva-x11-dev pkg-config\n\
         Fedora:          sudo dnf install libva-devel libva-drm-devel libva-wayland-devel libva-x11-devel pkg-config\n\
         Arch:            sudo pacman -S libva libva-mesa-driver\n\
         NixOS:           nix-shell shell.nix   (or set LIBVA_LIB_DIR to libva's lib directory)\n\
         \n\
         See vaapi-sys/README.md for details."
    );
}

fn generate_bindings() {
    let va = pkg_config::Config::new()
        .probe("libva")
        .expect("libva not found");

    let mut clang_args: Vec<String> = va
        .include_paths
        .iter()
        .map(|path| format!("-I{}", path.display()))
        .collect();

    if env::var("CARGO_FEATURE_DRM").is_ok() {
        let va_drm = pkg_config::Config::new()
            .probe("libva-drm")
            .expect("libva-drm not found");
        clang_args.extend(
            va_drm
                .include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        );
    }

    if env::var("CARGO_FEATURE_X11").is_ok() {
        let va_x11 = pkg_config::Config::new()
            .probe("libva-x11")
            .expect("libva-x11 not found");
        clang_args.extend(
            va_x11
                .include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        );
    }

    if env::var("CARGO_FEATURE_WAYLAND").is_ok() {
        let va_wl = pkg_config::Config::new()
            .probe("libva-wayland")
            .expect("libva-wayland not found");
        clang_args.extend(
            va_wl
                .include_paths
                .iter()
                .map(|path| format!("-I{}", path.display())),
        );
    }

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_args(&clang_args)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("va.*")
        .allowlist_type("VA.*")
        .allowlist_type("VASurfaceID")
        .allowlist_type("VABufferID")
        .allowlist_type("VAContextID")
        .allowlist_type("VAConfigID")
        .allowlist_type("VADisplay")
        .allowlist_type("VAImageID")
        .allowlist_type("VAGenericID")
        .allowlist_var("VA_.*")
        .blocklist_type("max_align_t")
        .size_t_is_usize(true)
        .wrap_unsafe_ops(true);

    if env::var("CARGO_FEATURE_DRM").is_ok() {
        builder = builder.clang_arg("-DCFG_vaapi_drm");
    }
    if env::var("CARGO_FEATURE_X11").is_ok() {
        builder = builder.clang_arg("-DCFG_vaapi_x11");
    }
    if env::var("CARGO_FEATURE_WAYLAND").is_ok() {
        builder = builder.clang_arg("-DCFG_vaapi_wayland");
    }

    let bindings = builder
        .generate()
        .expect("failed to generate VA-API bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}
