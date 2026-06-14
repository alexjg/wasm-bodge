{
  description = "Development environment for wasm-bodge";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };

          wasmBindgenCli = pkgs.buildWasmBindgenCli rec {
            src = pkgs.fetchCrate {
              pname = "wasm-bindgen-cli";
              version = "0.2.120";
              hash = "sha256-Dkkx8Bhfk+y/jEz9Fzwytmv2N3Gj/7ST+5MlPRzzetU=";
            };

            cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
              inherit src;
              inherit (src) pname version;
              hash = "sha256-5Zu/Sh9aBMxB+KGC1MHWJAQ8PuE40M6lsenkpFEwJ6A=";
            };
          };

          chromiumPackage = pkgs.lib.optional pkgs.stdenv.isLinux pkgs.chromium;
          chromiumBin = pkgs.lib.optionalString pkgs.stdenv.isLinux (pkgs.lib.getExe pkgs.chromium);
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              clippy
              rustfmt
              rust-analyzer
              lld

              wasmBindgenCli
              binaryen

              nodejs
              esbuild
              typescript
              wrangler

              pkg-config
              cacert
            ] ++ chromiumPackage;

            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
            CARGO_HTTP_CAINFO = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
            CARGO_NET_GIT_FETCH_WITH_CLI = "true";

            PUPPETEER_SKIP_DOWNLOAD = "1";
            PUPPETEER_EXECUTABLE_PATH = chromiumBin;
            CHROME_BIN = chromiumBin;

            shellHook = ''
              echo "wasm-bodge dev shell: rust, lld, wasm-bindgen 0.2.120, wasm-opt, node/npm, esbuild, tsc, wrangler"
            '';
          };
        });
    };
}
