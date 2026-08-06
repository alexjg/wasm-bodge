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
              version = "0.2.127";
              hash = "sha256-di+qBAdd7pENLiIB9CoZoab+W5xeDoByMREcCGTSzWo=";
            };

            cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
              inherit src;
              inherit (src) pname version;
              hash = "sha256-FTv2GZIAQs0ePdIZXIXil7JbZ6kIT05VG6vqC1qNFxQ=";
            };
          };

          chromiumPackage = pkgs.lib.optional pkgs.stdenv.isLinux pkgs.chromium;
          chromiumBin = pkgs.lib.optionalString pkgs.stdenv.isLinux (pkgs.lib.getExe pkgs.chromium);
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              # wasm-bodge's default panic=unwind build uses `cargo +nightly`
              # and therefore needs rustup's cargo proxy rather than nixpkgs'
              # standalone stable cargo/rustc packages.
              rustup
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
              echo "wasm-bodge dev shell: rustup, lld, wasm-bindgen 0.2.127, wasm-opt, node/npm, esbuild, tsc, wrangler"
              if ! rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
                echo "Install the default build toolchain with: rustup toolchain install nightly --component rust-src --component rustfmt --component clippy"
              fi
            '';
          };
        });
    };
}
