{
  description = "http server, building latest";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage rec {
          pname = "httplz";
          version = "2.4.0";

          src = pkgs.fetchFromGitHub {
            owner = "thecoshman";
            repo = "http";
            tag = "v${version}";
            hash = "sha256-Jd80ehOCBN62G4YjZOyTFYZ+ePOpPGpCE6S46arLbEc=";
          };

          cargoHash = "sha256-jKB6BObE3bu/VZemzV1BInw89r7kbrwSY/oJIjBzmxw=";

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];

          # Skip tests to speed up build
          doCheck = false;
        };
      }
    );
}
