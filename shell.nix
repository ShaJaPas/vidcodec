# Optional NixOS dev shell — required for `cargo build` on NixOS (non-FHS layout).
#
# FHS distros (Debian, Fedora, Arch): install libva dev packages; no shell needed.
#   apt install libva-dev libva-drm-dev libva-wayland-dev libva-x11-dev pkg-config
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    pkg-config
    clang
    llvmPackages.libclang
    libva
    libdrm
    xorg.libX11
    wayland
    mesa
    libva-utils
  ];

  # Mesa VA drivers (runtime; vainfo / probe tests on NixOS).
  LIBVA_DRIVER_NAME = "radeonsi";
  LIBVA_DRIVERS_PATH = "${pkgs.mesa.drivers}/lib/dri";

  shellHook = ''
    export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
  '';
}
