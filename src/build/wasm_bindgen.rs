use anyhow::{Context, Result};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use super::targets::{WasmBindgenTarget, WasmVariant};

/// Build wasm and run wasm-bindgen for all targets. When `debug_profile`
/// is `Some(name)`, also drives `cargo build --profile <name>` to produce
/// a parallel wasm with DWARF preserved.
pub fn build_wasm(
    crate_path: &Path,
    output_dir: &Path,
    release_profile: &str,
    debug_profile: Option<&str>,
    wasm_opt: bool,
) -> Result<()> {
    // Resolve `target_dir` and `wasm_name` once: both are invariant across
    // the release and debug builds, and each call to `find_target_dir`
    // spawns `cargo metadata` while `get_crate_name` reparses `Cargo.toml`.
    let target_dir = find_target_dir(crate_path)?;
    let wasm_name = get_crate_name(crate_path)?.replace('-', "_");

    println!("  Building Rust crate (profile: {release_profile})...");
    cargo_build(crate_path, release_profile)?;
    let release_wasm = wasm_artifact_path(&target_dir, &wasm_name, release_profile)?;

    let debug_wasm: Option<PathBuf> = match debug_profile {
        Some(profile) => {
            println!("  Building Rust crate (profile: {profile}, for debug variant)...");
            cargo_build_debug_profile(crate_path, profile)?;
            Some(wasm_artifact_path(&target_dir, &wasm_name, profile)?)
        }
        None => None,
    };

    if wasm_opt {
        println!("  Running wasm-opt on release variant...");
        run_wasm_opt(&release_wasm)?;
    }

    std::fs::create_dir_all(output_dir)?;

    for target in WasmBindgenTarget::all() {
        run_wasm_bindgen(&release_wasm, output_dir, *target, WasmVariant::Optimized)?;
    }

    if let Some(debug_wasm) = debug_wasm.as_deref() {
        for target in [WasmBindgenTarget::Web, WasmBindgenTarget::Bundler] {
            run_wasm_bindgen(debug_wasm, output_dir, target, WasmVariant::Debug)?;
        }
    }

    Ok(())
}

fn cargo_build(crate_path: &Path, profile: &str) -> Result<()> {
    let profile_arg = if profile == "release" {
        "--release".to_string()
    } else {
        format!("--profile={profile}")
    };

    let status = Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            &profile_arg,
            "--manifest-path",
            &crate_path.join("Cargo.toml").to_string_lossy(),
        ])
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("cargo build failed for profile `{profile}`");
    }
    Ok(())
}

/// Like `cargo_build`, but wraps cargo's "profile not defined" error with
/// a snippet users can paste into `Cargo.toml`.
fn cargo_build_debug_profile(crate_path: &Path, profile: &str) -> Result<()> {
    // Tee stderr so cargo's progress still streams live while we keep a
    // copy for post-hoc error classification.
    let mut child = Command::new("cargo")
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            &format!("--profile={profile}"),
            "--manifest-path",
            &crate_path.join("Cargo.toml").to_string_lossy(),
        ])
        .env("LANG", "C")
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn cargo build")?;

    let mut captured_stderr = Vec::new();
    if let Some(mut child_stderr) = child.stderr.take() {
        let mut buf = [0u8; 4096];
        loop {
            match child_stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    captured_stderr.extend_from_slice(&buf[..n]);
                    let _ = std::io::stderr().write_all(&buf[..n]);
                }
                Err(e) => return Err(e).context("Failed to read cargo stderr"),
            }
        }
    }

    let status = child.wait().context("Failed to wait on cargo build")?;
    if status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&captured_stderr);
    let needle = format!("profile `{profile}` is not defined");
    let profile_missing = stderr
        .lines()
        .any(|line| line.starts_with("error:") && line.contains(&needle));
    if profile_missing {
        anyhow::bail!(
            "--debug-profile {profile} requires a [profile.{profile}] section \
             in your Cargo.toml (or in the workspace root's Cargo.toml if this \
             crate is a workspace member -- cargo reads [profile.*] only from \
             the workspace root).\n\n\
             Recommended snippet:\n\n    \
             [profile.{profile}]\n    \
             inherits = \"dev\"\n    \
             debug = \"full\"\n    \
             opt-level = 0\n    \
             strip = \"none\"\n\n\
             Or pass --debug-profile <other-name> to use a profile you already have."
        );
    }

    anyhow::bail!("cargo build failed for profile `{profile}`")
}

fn wasm_artifact_path(target_dir: &Path, wasm_name: &str, profile: &str) -> Result<PathBuf> {
    let path = target_dir
        .join("wasm32-unknown-unknown")
        .join(profile_dir_name(profile))
        .join(format!("{wasm_name}.wasm"));

    if !path.exists() {
        anyhow::bail!("Wasm file not found at {path:?}");
    }
    Ok(path)
}

/// Cargo maps `dev`/`test` to `debug/` and `bench` to `release/`;
/// custom profiles use their own name.
fn profile_dir_name(profile: &str) -> &str {
    match profile {
        "dev" | "test" => "debug",
        "release" | "bench" => "release",
        other => other,
    }
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
