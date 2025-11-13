{ pkgs, lib, config, inputs, ... }:

{
  env.PORT = "8080"; # for output
  env.VIZ_PORT = "1111";
  env.TILES_PORT = "2222";
  
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
    # for serving compiled wasm
    pkgs.http-server

    # use flake in /martin-flake to build altest martin
    inputs.mart-update.packages.${pkgs.system}.default
  ];
  # get the real gamer command prompt
  starship.enable = true;
  
  # https://devenv.sh/languages/
  languages.rust.enable = true;
  languages.rust.channel = "nightly";
  languages.rust.targets = ["wasm32-unknown-unknown"];

  processes = {
    client-web = {
      cwd = "./diver_viz/build";
      # serve static content for client
      exec = "http-server -p ${config.env.VIZ_PORT}";
    };
    # client-dev = {
    #   cwd = "./diver_viz";
    #   # use trunk to build and serve the wasm version of the client
    #   # runs on port 1111 (see `diver_viz/Trunk.toml`)
    #   exec = "trunk serve";
    # };
    martin = {
      # run martin with default settings pointed at test Denver pmtiles data
      # runs on port 2222 (see `martin-config.yaml`)
      exec = ''
        martin --config martin/config.yaml
      '';
    };
  };

  services = {
    nginx = {
      enable = true;
      httpConfig = ''
           upstream backend_main {
                server localhost:${config.env.VIZ_PORT};
           }
    
           upstream backend_tiles {
                server localhost:${config.env.TILES_PORT};
           }
           server {
              listen ${config.env.PORT};
              server_name localhost;
        
              # Route tiles/* requests to tile server, stripping the tiles/ prefix
              location /tiles/ {
                  rewrite ^/tiles/(.*)$ /$1 break;
                  proxy_pass http://backend_tiles;
                  proxy_set_header Host $host;
                  proxy_set_header X-Real-IP $remote_addr;
                  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                  proxy_set_header X-Forwarded-Proto $scheme;
              }
        
              # Route all other requests to viz
              location / {
                  proxy_pass http://backend_main;
                  proxy_set_header Host $host;
                  proxy_set_header X-Real-IP $remote_addr;
                  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
                  proxy_set_header X-Forwarded-Proto $scheme;
              }
          } 
      '';
    };
  };

  containers.processes = {
    name = "devenv-denver-diver";
    version= "0.1.0";
    registry = "docker://us-west2-docker.pkg.dev/wasm-games-435303/diver/";
    copyToRoot = [
      ./diver_viz/build     # final built static html for client
      ./data                # static pmtiles data for martin to serve
      ./martin              # config for martin
    ];
  };

  scripts.build_and_deploy.exec = ''
    # build wasm app
    cd diver_viz
    trunk build --dist build
    cd ..
    # compile container and send to registry
    devenv container --profile prod copy processes
  '';

  profiles.prod.module = { config, ... }: {
    process.managers.process-compose = {
      port = config.env.PORT;
      tui.enable = false;
    };
    starship.enable = false;
  };
}
