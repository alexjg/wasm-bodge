use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::targets::WasmBindgenTarget;

/// Build wasm and run wasm-bindgen for all targets
pub fn build_wasm(crate_path: &Path, output_dir: &Path, profile: &str) -> Result<()> {
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
    let wasm_file = target_dir
        .join("wasm32-unknown-unknown")
        .join(profile_dir)
        .join(format!("{}.wasm", crate_name.replace('-', "_")));

    if !wasm_file.exists() {
        anyhow::bail!("Wasm file not found at {:?}", wasm_file);
    }

    // Run wasm-bindgen for each target defined in targets.rs
    std::fs::create_dir_all(output_dir)?;

    for target in WasmBindgenTarget::all() {
        println!("  Running wasm-bindgen for target '{}'...", target);
        let target_dir = output_dir.join(target.dir_name());
        std::fs::create_dir_all(&target_dir)?;

        let status = Command::new("wasm-bindgen")
            .args([
                &wasm_file.to_string_lossy(),
                "--out-dir",
                &target_dir.to_string_lossy(),
                "--target",
                target.as_str(),
                "--weak-refs",
            ])
            .status()
            .context("Failed to run wasm-bindgen")?;

        if !status.success() {
            anyhow::bail!("wasm-bindgen failed for target '{}'", target);
        }
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
