{ pkgs, lib, config, inputs, ... }:
{
  # https://devenv.sh/basics/
  env.LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
  env.BINDGEN_EXTRA_CLANG_ARGS = builtins.concatStringsSep " " [
    ''-I"${pkgs.glibc.dev}/include"''
    ''-I"${pkgs.clang_18}/resource-root/include"''
  ];
  env.RUST_BACKTRACE = 1;
  
  env.LD_LIBRARY_PATH = lib.makeLibraryPath [ 
    pkgs.llvm_18 
    pkgs.clang_18 
    pkgs.libclang.lib 
    pkgs.stdenv.cc.cc.lib 
    pkgs.vulkan-loader
    pkgs.xorg.libX11
    pkgs.xorg.libXi
    pkgs.xorg.libXcursor
    pkgs.libxkbcommon
    pkgs.wayland
  ];
  
  # https://devenv.sh/packages/
  packages = [
    ## general stuff to have for dev work
    pkgs.git # i love commiting
    pkgs.trunk # for doing wasm builds/local serving
    pkgs.lldb 
    pkgs.libclang 
    pkgs.gcc 
    pkgs.glibc.dev 
    pkgs.stdenv.cc.cc.lib
    # stuff copied from bevy nix stuff
    # Audio (Linux only)
    pkgs.alsa-lib
    # Cross Platform 3D Graphics API
    pkgs.vulkan-loader
    # For debugging around vulkan
    pkgs.vulkan-tools
    # Other dependencies
    pkgs.libudev-zero
    pkgs.xorg.libX11
    pkgs.xorg.libXcursor
    pkgs.xorg.libXi
    pkgs.xorg.libXrandr
    pkgs.libxkbcommon
    pkgs.wayland
  ];
  # get the real gamer command prompt
  starship.enable = true;
  
  # https://devenv.sh/languages/
  languages.rust.enable = true;
  languages.rust.channel = "nightly";
  languages.rust.targets = ["wasm32-unknown-unknown"];
  
  processes = {
    client-web = {
      cwd = "./diver_viz";
      # use trunk to build and serve the wasm version of the client
      exec = "trunk serve";
    };
    martin = {
      # run martin with default settings pointed at test Denver pmtiles data
      exec = "martin data/denver_blocks_all_zoom_15_up.pmtiles";
    };
  };
}
