## Unreleased

### Fixed

* Generated entrypoints now call `initSync({ module: bytes })` instead
  of the positional-bytes form `initSync(bytes)`, silencing the
  `using deprecated parameters for initSync()` warning emitted by
  wasm-bindgen 0.2.87+.

### Added

* `--release-profile <name>` CLI flag (alias of the existing `--profile`) for the release variant, default `release`.
* `--debug-profile <name>` CLI flag. Passing it drives a second `cargo build --profile <name>` and exposes the resulting wasm via `/debug/*` subpath exports. No default: if you don't pass the flag, no debug variant is built. With the recommended `inherits = "dev"` profile, the `/debug` variant is a full Rust dev build (DWARF + debug assertions + overflow checks + opt-level 0) with DWARF preserved through `wasm-bindgen --keep-debug`.

### Removed

* `--debug-variant` flag. Debug variants are now requested by passing `--debug-profile <name>` with an explicit profile name. See the Breaking Changes section for migration.

### Changed

* Debug variants are no longer produced by copying the already-compiled release wasm into a `/debug` slot. Previously the approach silently produced useless debug artifacts whenever the consumer's `[profile.release]` did not preserve DWARF (the Rust default). Only a dedicated profile gets DWARF, debug assertions, overflow checks, and low optimization into the packaged `/debug/*` output regardless of how the release profile is configured.

### Breaking Changes

* `--debug-variant` has been removed. The replacement is `--debug-profile <name>`: declare a `[profile.<name>]` section in the authoritative `Cargo.toml` (the workspace root for workspace members, or the crate's own manifest for standalone crates), then pass `--debug-profile <name>`. If the named profile is not declared, wasm-bodge fails with an error. If your v0.2.3 invocation was `wasm-bodge build --debug-variant` and it worked because `[profile.release]` had `debug = true`, you have two migration paths: (a) recommended — add a `[profile.wasm-debug]` section with `inherits = "dev"` for a proper debug build, then pass `--debug-profile wasm-debug`; (b) minimal — pass `--debug-profile release` to reuse the release profile. Option (b) preserves DWARF but not debug assertions, overflow checks, or recognizable variable scopes.

## 0.2.3 - 17th April 2026

* Add a --debug-variant flag which builds an additional /debug export which
  includes DWARF symbols in the WebAssembly

## 0.2.2 - 27th march 2026

### Fixed

* Entrypoint files were being omitted from package.json sideEffects which
  meant that bundlers would tree shake out the initiailzation code

## 0.2.1 - 5th march 2026

### Fixed

* Package names containing a scope would fail to build as `wasm-bodge` would
  attempt to write the webassemblyt output to `/dist/scope/<wasm filename>`

## 0.2.0 - 4th March 2026 

### Added

* Now runs `wasm-opt` as part of the build

### Fixed

* A bug where code which imported from the `/slim` export and then later
  initialized the WebAssembly elsewhere would fail to initialize the
  WebAssembly referenced by the `/slim` export.
