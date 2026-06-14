use std::path::PathBuf;

/// Configuration for the build command
#[derive(Debug)]
pub struct BuildConfig {
    pub crate_path: PathBuf,
    pub package_json: PathBuf,
    pub out_dir: PathBuf,
    pub release_profile: String,
    pub debug_profile: Option<String>,
    pub wasm_bindgen_tar: Option<PathBuf>,
    pub wasm_opt: bool,
}

/// Optional TypeScript wrapper configuration read from package.json under
/// `wasm-bodge.wrapper`.
#[derive(Debug, Clone)]
pub struct WrapperConfig {
    /// TypeScript entrypoint for the high-level root export.
    pub entry: PathBuf,
    /// Optional TypeScript entrypoint for the high-level `./slim` export.
    /// If omitted, `entry` is compiled again against the slim raw bindings.
    pub slim_entry: Option<PathBuf>,
    /// Optional tsconfig to use when bundling and typechecking wrapper sources.
    /// If omitted, wasm-bodge uses the nearest existing tsconfig.json when one
    /// can be found.
    pub tsconfig: Option<PathBuf>,
    /// Whether to expose the raw generated wasm-bindgen API under
    /// `./bindings` and `./bindings/slim`.
    pub expose_bindings: bool,
    /// Whether esbuild should emit source maps for wrapper JavaScript.
    pub source_map: bool,
}
