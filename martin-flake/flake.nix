{
  description = "Martin tile server - latest version without webui";

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
          pname = "martin";
          version = "0.20.2";

          src = pkgs.fetchFromGitHub {
            owner = "maplibre";
            repo = "martin";
            tag = "martin-v${version}";
            hash = "sha256-Jd80ehOCBN62G4YjZOyTFYZ+ePOpPGpCE6S46arLbEc=";
          };

          cargoHash = "sha256-jKB6BObE3bu/VZemzV1BInw89r7kbrwSY/oJIjBzmxw=";

          nativeBuildInputs = [ pkgs.pkg-config  pkgs.nodejs ];
          buildInputs = [ pkgs.openssl ];

          # Skip tests to speed up build
          doCheck = false;

          meta = with pkgs.lib; {
            description = "Blazing fast and lightweight PostGIS vector tiles server";
            homepage = "https://martin.maplibre.org/";
            license = with licenses; [ mit asl20 ];
          };
        };
      }
    );
}