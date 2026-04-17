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
