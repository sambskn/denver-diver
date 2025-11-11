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
            tag = "v${version}";
            hash = "sha256-7VCAHhAAe2FgesecongratsyougettorunNixtwice=";
          };

          useFetchCargoVendor = true;
          cargoHash = "sha256-fillmein12345678901234567890123456789012=";

          buildFeatures = [
            "fonts"
            "lambda"
            "mbtiles"
            "metrics"
            "pmtiles"
            "postgres"
            "sprites"
            "styles"
            # webui excluded
          ];

          nativeBuildInputs = [ pkgs.pkg-config ];
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