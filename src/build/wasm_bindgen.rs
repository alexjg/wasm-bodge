use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::targets::{WasmBindgenTarget, WasmVariant};

/// Build wasm and run wasm-bindgen for all targets. When `debug_variant` is
/// true, also builds a parallel debug variant (DWARF preserved) under
/// `web-debug/` and `bundler-debug/`.
pub fn build_wasm(
    crate_path: &Path,
    output_dir: &Path,
    profile: &str,
    wasm_opt: bool,
    debug_variant: bool,
) -> Result<()> {
    // Build the Rust crate
    println!("  Building Rust crate...");
    let cargo_profile = if profile == "release" {
        "--release"
    } else {
        &format!("--profile={}", profile)
    };

    let status = Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            cargo_profile,
            "--manifest-path",
            &crate_path.join("Cargo.toml").to_string_lossy(),
        ])
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("cargo build failed");
    }

    // Find the wasm file
    let target_dir = find_target_dir(crate_path)?;
    let profile_dir = if profile == "release" {
        "release"
    } else {
        profile
    };
    let crate_name = get_crate_name(crate_path)?;
    let wasm_name = crate_name.replace('-', "_");
    let wasm_file = target_dir
        .join("wasm32-unknown-unknown")
        .join(profile_dir)
        .join(format!("{}.wasm", wasm_name));

    if !wasm_file.exists() {
        anyhow::bail!("Wasm file not found at {:?}", wasm_file);
    }

    // If a debug variant is requested, copy the source wasm (with DWARF) to a
    // sibling subdirectory so the debug wasm-bindgen run has its own pristine
    // input. Preserving the file stem keeps wasm-bindgen's output filenames
    // consistent across variants.
    //
    // We do NOT run wasm-opt on the debug variant: binaryen's DWARF support is
    // incomplete and cannot process the DWARF that wasm-bindgen (walrus)
    // rewrites, so any `wasm-opt -g` pass on wasm-bindgen's `--keep-debug`
    // output fails with a `debug_loc error`. Running wasm-opt without `-g`
    // would strip debug symbols, defeating the point of the debug variant.
    let debug_wasm_file = if debug_variant {
        let debug_wasm_dir = wasm_file.parent().unwrap().join("_wasm_bodge_debug");
        std::fs::create_dir_all(&debug_wasm_dir)?;
        let path = debug_wasm_dir.join(format!("{}.wasm", wasm_name));
        std::fs::copy(&wasm_file, &path).context("Failed to copy wasm for debug variant")?;
        Some(path)
    } else {
        None
    };

    if wasm_opt {
        println!("  Running wasm-opt (optimized, strips debug symbols)...");
        run_wasm_opt(&wasm_file)?;
    }

    std::fs::create_dir_all(output_dir)?;

    // Optimized variant: run wasm-bindgen for all targets (nodejs, web, bundler)
    for target in WasmBindgenTarget::all() {
        run_wasm_bindgen(&wasm_file, output_dir, *target, WasmVariant::Optimized)?;
    }

    // Debug variant: run wasm-bindgen --keep-debug for web + bundler. No
    // wasm-opt pass (see above).
    if let Some(debug_wasm_file) = debug_wasm_file.as_deref() {
        for target in [WasmBindgenTarget::Web, WasmBindgenTarget::Bundler] {
            run_wasm_bindgen(debug_wasm_file, output_dir, target, WasmVariant::Debug)?;
        }
    }

    Ok(())
}

fn run_wasm_opt(wasm_file: &Path) -> Result<()> {
    let wasm_path = wasm_file.to_string_lossy();
    let status = Command::new("wasm-opt")
        .args(["-O4", "--all-features", "-o", &wasm_path, &wasm_path])
        .status()
        .context("Failed to run wasm-opt. Is it installed? (cargo install wasm-opt)")?;

    if !status.success() {
        anyhow::bail!("wasm-opt failed");
    }
    Ok(())
}

fn run_wasm_bindgen(
    wasm_file: &Path,
    output_dir: &Path,
    target: WasmBindgenTarget,
    variant: WasmVariant,
) -> Result<()> {
    let dir_name = format!("{}{}", target.dir_name(), variant.dir_suffix());
    println!(
        "  Running wasm-bindgen for target '{}' ({})...",
        target,
        if variant.is_debug() {
            "debug"
        } else {
            "optimized"
        }
    );
    let target_dir = output_dir.join(&dir_name);
    std::fs::create_dir_all(&target_dir)?;

    let mut cmd = Command::new("wasm-bindgen");
    cmd.args([
        &wasm_file.to_string_lossy(),
        "--out-dir",
        &target_dir.to_string_lossy(),
        "--target",
        target.as_str(),
        "--weak-refs",
    ]);
    if variant.is_debug() {
        cmd.arg("--keep-debug");
    }
    let status = cmd.status().context("Failed to run wasm-bindgen")?;

    if !status.success() {
        anyhow::bail!("wasm-bindgen failed for target '{}' ({})", target, dir_name);
    }
    Ok(())
}

fn find_target_dir(crate_path: &Path) -> Result<PathBuf> {
    // First check for workspace target dir by looking at cargo metadata
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version=1",
            "--no-deps",
            "--manifest-path",
            &crate_path.join("Cargo.toml").to_string_lossy(),
        ])
        .output()
        .context("Failed to run cargo metadata")?;

    if output.status.success() {
        let metadata: serde_json::Value =
            serde_json::from_slice(&output.stdout).context("Failed to parse cargo metadata")?;

        if let Some(target_dir) = metadata["target_directory"].as_str() {
            return Ok(PathBuf::from(target_dir));
        }
    }

    // Fallback to crate-local target dir
    Ok(crate_path.join("target"))
}

fn get_crate_name(crate_path: &Path) -> Result<String> {
    let cargo_toml_path = crate_path.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml_path).context("Failed to read Cargo.toml")?;

    let parsed: toml::Value = toml::from_str(&content).context("Failed to parse Cargo.toml")?;

    parsed["package"]["name"]
        .as_str()
        .map(String::from)
        .context("Could not find package name in Cargo.toml")
}
