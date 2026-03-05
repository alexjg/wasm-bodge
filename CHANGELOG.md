## Unreleased

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
