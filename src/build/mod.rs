use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::build::targets::WasmVariant;
use crate::config::BuildConfig;

mod entrypoints;
mod finalize;
mod package_json;
mod post_process;
pub mod targets;
mod wasm_bindgen;

/// Main build orchestrator
pub fn run(config: BuildConfig) -> Result<()> {
    println!("wasm-bodge build starting...");

    let crate_path = &config.crate_path;

    // Create output directory
    std::fs::create_dir_all(&config.out_dir).context("Failed to create output directory")?;

    let wasm_bindgen_dir = config.out_dir.join("wasm_bindgen");

    // Phase 1: Build wasm or extract from tarball
    if let Some(tarball) = &config.wasm_bindgen_tar {
        println!("Extracting prebuilt wasm-bindgen output from {:?}", tarball);
        extract_tarball(tarball, &wasm_bindgen_dir)?;
    } else {
        println!("Phase 1: Building wasm...");
        wasm_bindgen::build_wasm(
            crate_path,
            &wasm_bindgen_dir,
            &config.release_profile,
            config.debug_profile.as_deref(),
            config.wasm_opt,
        )?;
    }

    // Get crate name from Cargo.toml
    let crate_name = get_crate_name(crate_path)?;
    println!("Crate name: {}", crate_name);

    // Get package name from package.json (or derive from crate name)
    let package_name = get_package_name(&config.package_json, &crate_name)?;

    // Phase 2: Post-process
    println!("Phase 2: Post-processing...");
    post_process::run(&wasm_bindgen_dir, &config.out_dir, &crate_name)?;

    // Phase 3: Generate entrypoints
    println!("Phase 3: Generating entrypoints...");
    entrypoints::generate(&config.out_dir, &crate_name)?;

    // Phase 4: Finalize package
    println!("Phase 4: Finalizing package...");
    let available_variants = if config.debug_profile.is_some() {
        WasmVariant::all()
    } else {
        &[WasmVariant::Optimized]
    };
    finalize::run(
        &config.package_json,
        &config.out_dir,
        &crate_name,
        &package_name,
        available_variants,
    )?;

    println!("Build complete! Output in {:?}", config.out_dir);
    Ok(())
}

fn extract_tarball(tarball: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let status = Command::new("tar")
        .args([
            "-xzf",
            &tarball.to_string_lossy(),
            "-C",
            &dest.to_string_lossy(),
        ])
        .status()
        .context("Failed to run tar")?;

    if !status.success() {
        anyhow::bail!("tar extraction failed");
    }
    Ok(())
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

fn get_package_name(package_json_path: &Path, crate_name: &str) -> Result<String> {
    let content =
        std::fs::read_to_string(package_json_path).context("Failed to read package.json")?;
    let parsed: serde_json::Value =
        serde_json::from_str(&content).context("Failed to parse package.json")?;

    let name = parsed["name"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| crate_name.replace('_', "-"));

    // Strip npm scope (e.g. "@scope/name" -> "name") since the package name
    // is used to construct file paths like {name}.wasm, not as the npm name.
    Ok(match name.split_once('/') {
        Some((_, unscoped)) => unscoped.to_string(),
        None => name,
    })
}
