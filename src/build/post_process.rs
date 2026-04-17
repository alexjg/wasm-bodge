use anyhow::{Context, Result};
use base64::Engine;
use regex::Regex;
use std::path::Path;

use super::targets::{self, WasmBindgenTarget, WasmVariant};

/// Post-process wasm-bindgen output:
/// 1. Rename nodejs output .js to .cjs (since package uses "type": "module")
/// 2. For each variant's web target: apply @vite-ignore fix and add
///    `__wbg_set_wasm` export
/// 3. Generate a base64 wasm module for each variant
pub fn run(wasm_bindgen_dir: &Path, out_dir: &Path, crate_name: &str) -> Result<()> {
    // Normalize crate name (Rust uses underscores in generated files)
    let wasm_name = crate_name.replace('-', "_");

    // 1. Rename nodejs .js to .cjs (wasm-bindgen nodejs target outputs CJS).
    //    Only the optimized variant has a nodejs target.
    println!("  Renaming nodejs .js to .cjs...");
    let nodejs_dir = wasm_bindgen_dir.join(WasmBindgenTarget::Nodejs.dir_name());
    let js_file = nodejs_dir.join(format!("{}.js", wasm_name));
    let cjs_file = nodejs_dir.join(format!("{}.cjs", wasm_name));
    if js_file.exists() {
        std::fs::rename(&js_file, &cjs_file)?;
    }

    // 2 & 3. Process each variant's web target (if present).
    for variant in WasmVariant::all() {
        let web_dir = wasm_bindgen_dir.join(format!("web{}", variant.dir_suffix()));
        if !web_dir.exists() {
            continue;
        }

        println!("  Applying @vite-ignore fix to {}...", web_dir.display());
        apply_vite_fix(&web_dir, &wasm_name)?;

        println!("  Adding __wbg_set_wasm export to {}...", web_dir.display());
        add_set_wasm_export(&web_dir, &wasm_name)?;

        println!(
            "  Generating base64 wasm module for {} variant...",
            if variant.is_debug() {
                "debug"
            } else {
                "optimized"
            }
        );
        generate_base64_module(&web_dir, out_dir, &wasm_name, *variant)?;
    }

    Ok(())
}

fn apply_vite_fix(web_dir: &Path, wasm_name: &str) -> Result<()> {
    let js_file = web_dir.join(format!("{}.js", wasm_name));
    let content =
        std::fs::read_to_string(&js_file).context("Failed to read wasm-bindgen JS file")?;

    // Replace: new URL('{name}_bg.wasm', import.meta.url)
    // With:    new /* @vite-ignore */ URL('{name}_bg.wasm', import.meta.url)
    let pattern = format!(r"new URL\('{}_bg\.wasm', import\.meta\.url\)", wasm_name);
    let replacement = format!(
        "new /* @vite-ignore */ URL('{}_bg.wasm', import.meta.url)",
        wasm_name
    );

    let re = Regex::new(&pattern)?;
    let new_content = re.replace_all(&content, replacement.as_str());

    std::fs::write(&js_file, new_content.as_ref()).context("Failed to write modified JS file")?;

    Ok(())
}

fn add_set_wasm_export(web_dir: &Path, wasm_name: &str) -> Result<()> {
    let js_file = web_dir.join(format!("{}.js", wasm_name));
    let mut content =
        std::fs::read_to_string(&js_file).context("Failed to read wasm-bindgen web JS file")?;

    content.push_str("\nexport function __wbg_set_wasm(val) { wasm = val; }\n");

    std::fs::write(&js_file, &content).context("Failed to write modified web JS file")?;
    Ok(())
}

fn generate_base64_module(
    web_dir: &Path,
    out_dir: &Path,
    wasm_name: &str,
    variant: WasmVariant,
) -> Result<()> {
    let wasm_file = web_dir.join(format!("{}_bg.wasm", wasm_name));
    let wasm_bytes = std::fs::read(&wasm_file).context("Failed to read wasm file")?;

    let base64_string = base64::engine::general_purpose::STANDARD.encode(&wasm_bytes);

    // Create esm directory and write the base64 module
    let esm_base64_path = out_dir.join(targets::paths::wasm_base64_esm(variant));
    if let Some(parent) = esm_base64_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let esm_content = format!("export const wasmBase64 = \"{}\";\n", base64_string);
    std::fs::write(&esm_base64_path, esm_content)?;

    Ok(())
}
