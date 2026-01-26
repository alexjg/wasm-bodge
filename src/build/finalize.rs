use anyhow::{Context, Result};
use std::path::Path;

use super::targets::{self, WasmBindgenTarget};

/// Finalize the build by:
/// 1. Updating package.json with generated exports
/// 2. Copying .d.ts to out_dir
/// 3. Copying .wasm to out_dir
/// 4. Generating CJS base64 module
pub fn run(
    package_json_path: &Path,
    out_dir: &Path,
    crate_name: &str,
    package_name: &str,
) -> Result<()> {
    let wasm_name = crate_name.replace('-', "_");

    // Compute relative path from package.json directory to out_dir
    let package_dir = package_json_path
        .parent()
        .context("package.json has no parent directory")?
        .canonicalize()
        .context("Failed to canonicalize package.json directory")?;
    let out_dir_abs = out_dir
        .canonicalize()
        .context("Failed to canonicalize out_dir")?;
    let out_dir_rel = pathdiff::diff_paths(&out_dir_abs, &package_dir)
        .context("Failed to compute relative path from package.json to out_dir")?;

    // Update package.json
    super::package_json::update(package_json_path, &out_dir_rel, package_name)?;

    // Copy .d.ts from nodejs target to out_dir
    copy_types(out_dir, &wasm_name, &out_dir_rel)?;

    // Copy .wasm from web target to out_dir
    copy_wasm(out_dir, &wasm_name, package_name, &out_dir_rel)?;

    // Generate CJS base64 module
    generate_cjs_base64(out_dir, &out_dir_rel)?;

    Ok(())
}

fn copy_types(out_dir: &Path, wasm_name: &str, out_dir_rel: &Path) -> Result<()> {
    let dts_src = out_dir
        .join(targets::paths::wasm_bindgen_dir(WasmBindgenTarget::Nodejs))
        .join(format!("{}.d.ts", wasm_name));
    let dts_dest = out_dir.join(targets::paths::types());

    if dts_src.exists() {
        std::fs::copy(&dts_src, &dts_dest)?;
        println!(
            "  Copied type declarations to {}/{}",
            out_dir_rel.display(),
            targets::paths::types().display()
        );
    }
    Ok(())
}

fn copy_wasm(
    out_dir: &Path,
    wasm_name: &str,
    package_name: &str,
    out_dir_rel: &Path,
) -> Result<()> {
    let wasm_src = out_dir
        .join(targets::paths::wasm_bindgen_dir(WasmBindgenTarget::Web))
        .join(format!("{}_bg.wasm", wasm_name));
    let wasm_dest = out_dir.join(targets::paths::standalone_wasm(package_name));

    if wasm_src.exists() {
        std::fs::copy(&wasm_src, &wasm_dest)?;
        println!(
            "  Copied wasm to {}/{}",
            out_dir_rel.display(),
            targets::paths::standalone_wasm(package_name).display()
        );
    }
    Ok(())
}

fn generate_cjs_base64(out_dir: &Path, out_dir_rel: &Path) -> Result<()> {
    let esm_base64_path = out_dir.join(targets::paths::wasm_base64_esm());
    let esm_base64 = std::fs::read_to_string(&esm_base64_path)?;

    // Extract the base64 string from the ESM module
    let base64_str = esm_base64
        .split('"')
        .nth(1)
        .context("Failed to parse base64 from ESM module")?;

    let cjs_base64_content = format!("module.exports.wasmBase64 = \"{}\";\n", base64_str);
    std::fs::write(
        out_dir.join(targets::paths::wasm_base64_cjs()),
        cjs_base64_content,
    )?;

    println!(
        "  Generated {}/{}",
        out_dir_rel.display(),
        targets::paths::wasm_base64_cjs().display()
    );
    Ok(())
}
