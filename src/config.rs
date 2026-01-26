use std::path::PathBuf;

/// Configuration for the build command
#[derive(Debug)]
pub struct BuildConfig {
    pub crate_path: PathBuf,
    pub package_json: PathBuf,
    pub out_dir: PathBuf,
    pub profile: String,
    pub wasm_bindgen_tar: Option<PathBuf>,
}
