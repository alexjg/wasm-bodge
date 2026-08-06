use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use crate::config::PanicStrategy;

use super::targets::{WasmBindgenTarget, WasmVariant};

#[derive(Debug, Clone, Copy)]
struct CargoBuildOptions<'a> {
    panic_strategy: PanicStrategy,
    rust_toolchain: Option<&'a str>,
}

/// Build wasm and run wasm-bindgen for all targets. When `debug_profile`
/// is `Some(name)`, also drives `cargo build --profile <name>` to produce
/// a parallel wasm with DWARF preserved.
pub fn build_wasm(
    crate_path: &Path,
    output_dir: &Path,
    release_profile: &str,
    debug_profile: Option<&str>,
    wasm_opt: bool,
    panic_strategy: PanicStrategy,
    rust_toolchain: Option<&str>,
) -> Result<()> {
    // Resolve `target_dir` and `wasm_name` once: both are invariant across
    // the release and debug builds, and each call to `find_target_dir`
    // spawns `cargo metadata` while `get_crate_name` reparses `Cargo.toml`.
    let target_dir = find_target_dir(crate_path)?;
    let wasm_name = get_crate_name(crate_path)?.replace('-', "_");

    let cargo_options = CargoBuildOptions {
        panic_strategy,
        rust_toolchain,
    };

    println!("  Building Rust crate (profile: {release_profile})...");
    cargo_build(crate_path, release_profile, cargo_options)?;
    let release_wasm = wasm_artifact_path(&target_dir, &wasm_name, release_profile)?;

    let debug_wasm: Option<PathBuf> = match debug_profile {
        Some(profile) => {
            println!("  Building Rust crate (profile: {profile}, for debug variant)...");
            cargo_build_debug_profile(crate_path, profile, cargo_options)?;
            Some(wasm_artifact_path(&target_dir, &wasm_name, profile)?)
        }
        None => None,
    };

    std::fs::create_dir_all(output_dir)?;

    // Run wasm-opt on wasm-bindgen's finalized `*_bg.wasm`
    for target in WasmBindgenTarget::all() {
        let bindgen_wasm = run_wasm_bindgen(
            &release_wasm,
            output_dir,
            *target,
            WasmVariant::Optimized,
            panic_strategy,
        )?;
        if wasm_opt {
            println!(
                "  Running wasm-opt on release variant ({} target)...",
                target
            );
            run_wasm_opt(&bindgen_wasm)?;
        }
    }

    if let Some(debug_wasm) = debug_wasm.as_deref() {
        run_wasm_bindgen(
            debug_wasm,
            output_dir,
            WasmBindgenTarget::Web,
            WasmVariant::Debug,
            panic_strategy,
        )?;
    }

    Ok(())
}

fn cargo_build(crate_path: &Path, profile: &str, options: CargoBuildOptions<'_>) -> Result<()> {
    let status = cargo_build_command(crate_path, profile, options)
        .status()
        .with_context(|| cargo_spawn_context(options))?;

    if !status.success() {
        bail_cargo_failure(profile, options)?;
    }
    Ok(())
}

/// Like `cargo_build`, but wraps cargo's "profile not defined" error with
/// a snippet users can paste into `Cargo.toml`.
fn cargo_build_debug_profile(
    crate_path: &Path,
    profile: &str,
    options: CargoBuildOptions<'_>,
) -> Result<()> {
    // Tee stderr so cargo's progress still streams live while we keep a
    // copy for post-hoc error classification.
    let mut command = cargo_build_command(crate_path, profile, options);
    let mut child = command
        .env("LANG", "C")
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| cargo_spawn_context(options))?;

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

    bail_cargo_failure(profile, options)
}

fn cargo_build_command(
    crate_path: &Path,
    profile: &str,
    options: CargoBuildOptions<'_>,
) -> Command {
    let mut command = Command::new("cargo");
    if let Some(toolchain) = effective_toolchain(options) {
        command.arg(format!("+{toolchain}"));
    }

    command
        .arg("build")
        .args(["--target", "wasm32-unknown-unknown"]);
    if profile == "release" {
        command.arg("--release");
    } else {
        command.arg(format!("--profile={profile}"));
    }
    command
        .arg("--manifest-path")
        .arg(crate_path.join("Cargo.toml"));

    if options.panic_strategy == PanicStrategy::Unwind {
        command.arg("-Zbuild-std=std,panic_unwind");
    }

    let (name, value) = rustflags_env(options.panic_strategy);
    command.env(name, value);
    command
}

fn effective_toolchain(options: CargoBuildOptions<'_>) -> Option<&str> {
    options.rust_toolchain.or(match options.panic_strategy {
        PanicStrategy::Unwind => Some("nightly"),
        PanicStrategy::Abort => None,
    })
}

fn rustflags_for(strategy: PanicStrategy) -> &'static [&'static str] {
    match strategy {
        PanicStrategy::Unwind => &["-Cpanic=unwind", "-Cllvm-args=-wasm-use-legacy-eh=true"],
        PanicStrategy::Abort => &["-Cpanic=abort"],
    }
}

fn append_rustflags(existing: Option<&OsStr>, flags: &[&str], separator: &str) -> OsString {
    let mut result = existing.unwrap_or_default().to_os_string();
    for flag in flags {
        if !result.is_empty() {
            result.push(separator);
        }
        result.push(flag);
    }
    result
}

fn rustflags_env(strategy: PanicStrategy) -> (&'static str, OsString) {
    const ENCODED: &str = "CARGO_ENCODED_RUSTFLAGS";
    if let Some(existing) = std::env::var_os(ENCODED) {
        return (
            ENCODED,
            append_rustflags(Some(&existing), rustflags_for(strategy), "\x1f"),
        );
    }

    const PLAIN: &str = "RUSTFLAGS";
    let existing = std::env::var_os(PLAIN);
    (
        PLAIN,
        append_rustflags(existing.as_deref(), rustflags_for(strategy), " "),
    )
}

fn cargo_spawn_context(options: CargoBuildOptions<'_>) -> String {
    match options.panic_strategy {
        PanicStrategy::Unwind => format!(
            "Failed to run cargo with Rust toolchain `{}`. panic=unwind requires a nightly \
             toolchain with rust-src; install it with `rustup toolchain install {} \
             --component rust-src`",
            effective_toolchain(options).unwrap_or("nightly"),
            effective_toolchain(options).unwrap_or("nightly"),
        ),
        PanicStrategy::Abort => "Failed to run cargo build".to_string(),
    }
}

fn bail_cargo_failure(profile: &str, options: CargoBuildOptions<'_>) -> Result<()> {
    match options.panic_strategy {
        PanicStrategy::Unwind => {
            let toolchain = effective_toolchain(options).unwrap_or("nightly");
            anyhow::bail!(
                "cargo build failed for profile `{profile}` with panic=unwind. This mode requires \
                 a nightly toolchain with rust-src and wasm-bindgen's `std` feature enabled. \
                 Install the toolchain with `rustup toolchain install {toolchain} --component \
                 rust-src`"
            )
        }
        PanicStrategy::Abort => anyhow::bail!("cargo build failed for profile `{profile}`"),
    }
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

fn wasm_opt_command(wasm_file: &Path) -> Command {
    let wasm_path = wasm_file.to_string_lossy();
    // Respect the target-features section emitted by wasm-bindgen. Passing
    // --all-features lets Binaryen introduce newer Wasm features which older
    // supported runtimes (notably Node 20) cannot validate.
    let mut command = Command::new("wasm-opt");
    command.args(["-O4", "-o", &wasm_path, &wasm_path]);
    command
}

fn run_wasm_opt(wasm_file: &Path) -> Result<()> {
    let status = wasm_opt_command(wasm_file)
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
    panic_strategy: PanicStrategy,
) -> Result<PathBuf> {
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

    if panic_strategy == PanicStrategy::Unwind && target == WasmBindgenTarget::Web {
        validate_legacy_exception_handling(&target_dir, wasm_file)?;
    }

    let wasm_stem = wasm_file
        .file_stem()
        .context("Cargo Wasm artifact has no file stem")?;
    let bindgen_wasm = target_dir.join(format!("{}_bg.wasm", wasm_stem.to_string_lossy()));
    if !bindgen_wasm.exists() {
        anyhow::bail!(
            "wasm-bindgen output Wasm not found at {}",
            bindgen_wasm.display()
        );
    }
    Ok(bindgen_wasm)
}

fn validate_legacy_exception_handling(output_dir: &Path, wasm_file: &Path) -> Result<()> {
    let js_file = std::fs::read_dir(output_dir)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "js"))
        .context("wasm-bindgen did not produce JavaScript output")?;
    let source = std::fs::read_to_string(&js_file)
        .with_context(|| format!("Failed to read {}", js_file.display()))?;

    // Nightlies from 2026-05-07 until the fix for rust-lang/rust#156554
    // reached the channel put the target's modern-EH LLVM argument after the
    // user's legacy-EH override. Detect that output rather than silently
    // publishing a package which fails in Node 20 and current Chromium.
    if uses_modern_exception_handling(&source) {
        anyhow::bail!(
            "the selected Rust nightly emitted modern exnref exception handling for {} even \
             though wasm-bodge requested legacy EH. Use nightly-2026-05-17 or newer (the \
             compiler must include rust-lang/rust#156554)",
            wasm_file.display()
        );
    }
    Ok(())
}

fn uses_modern_exception_handling(source: &str) -> bool {
    source.contains("__wbindgen_jstag: WebAssembly.JSTag")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn command_args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn wasm_opt_uses_only_declared_module_features() {
        let args = command_args(&wasm_opt_command(Path::new("module.wasm")));
        assert_eq!(args, ["-O4", "-o", "module.wasm", "module.wasm"]);
        assert!(!args.iter().any(|arg| arg == "--all-features"));
    }

    #[test]
    fn unwind_build_uses_nightly_build_std_and_profile() {
        let command = cargo_build_command(
            Path::new("crate"),
            "release",
            CargoBuildOptions {
                panic_strategy: PanicStrategy::Unwind,
                rust_toolchain: None,
            },
        );
        let args = command_args(&command);

        assert_eq!(args.first().map(String::as_str), Some("+nightly"));
        assert!(args.iter().any(|arg| arg == "--release"));
        assert!(args.iter().any(|arg| arg == "-Zbuild-std=std,panic_unwind"));
    }

    #[test]
    fn unwind_build_honors_selected_toolchain() {
        let command = cargo_build_command(
            Path::new("crate"),
            "wasm-debug",
            CargoBuildOptions {
                panic_strategy: PanicStrategy::Unwind,
                rust_toolchain: Some("nightly-2099-01-01"),
            },
        );
        let args = command_args(&command);

        assert_eq!(
            args.first().map(String::as_str),
            Some("+nightly-2099-01-01")
        );
        assert!(args.iter().any(|arg| arg == "--profile=wasm-debug"));
    }

    #[test]
    fn abort_build_does_not_select_nightly_or_build_std() {
        let command = cargo_build_command(
            Path::new("crate"),
            "release",
            CargoBuildOptions {
                panic_strategy: PanicStrategy::Abort,
                rust_toolchain: None,
            },
        );
        let args = command_args(&command);

        assert_eq!(args.first().map(String::as_str), Some("build"));
        assert!(!args.iter().any(|arg| arg.starts_with("+nightly")));
        assert!(!args.iter().any(|arg| arg.starts_with("-Zbuild-std")));
    }

    #[test]
    fn appends_plain_rustflags_without_discarding_existing_flags() {
        let result = append_rustflags(
            Some(OsStr::new("-Ctarget-feature=+simd128")),
            rustflags_for(PanicStrategy::Unwind),
            " ",
        );
        assert_eq!(
            result,
            OsString::from(
                "-Ctarget-feature=+simd128 -Cpanic=unwind \
                 -Cllvm-args=-wasm-use-legacy-eh=true"
            )
        );
    }

    #[test]
    fn appends_encoded_rustflags_without_discarding_existing_flags() {
        let result = append_rustflags(
            Some(OsStr::new("-Ctarget-feature=+simd128")),
            rustflags_for(PanicStrategy::Unwind),
            "\x1f",
        );
        assert_eq!(
            result,
            OsString::from(
                "-Ctarget-feature=+simd128\x1f-Cpanic=unwind\x1f\
                 -Cllvm-args=-wasm-use-legacy-eh=true"
            )
        );
    }

    #[test]
    fn distinguishes_modern_and_legacy_exception_handling_glue() {
        assert!(uses_modern_exception_handling(
            "__wbindgen_jstag: WebAssembly.JSTag,"
        ));
        assert!(!uses_modern_exception_handling(
            "__wbindgen_jstag: __wbindgen_jstag_polyfill,"
        ));
    }
}
