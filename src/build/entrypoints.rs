use anyhow::{Context, Result};
use heck::ToPascalCase;
use std::path::Path;
use std::process::Command;

use super::targets::{self, Environment};

/// Generate all entrypoints (ESM, CJS, IIFE)
pub fn generate(out_dir: &Path, crate_name: &str) -> Result<()> {
    let wasm_name = crate_name.replace('-', "_");
    let esm_dir = out_dir.join("esm");
    let cjs_dir = out_dir.join("cjs");
    let iife_dir = out_dir.join("iife");

    std::fs::create_dir_all(&esm_dir)?;
    std::fs::create_dir_all(&cjs_dir)?;
    std::fs::create_dir_all(&iife_dir)?;

    // Generate entrypoints for each environment defined in targets.rs
    println!("  Generating ESM entrypoints...");
    for env in Environment::all() {
        let content = targets::generate_esm_entrypoint(*env, &wasm_name);
        let path = esm_dir.join(format!("{}.js", env.file_stem()));
        std::fs::write(&path, content)?;
    }

    // Generate CJS entrypoints (only for environments that don't need bundling)
    println!("  Generating CJS entrypoints...");
    for env in Environment::all() {
        if let Some(content) = targets::generate_cjs_entrypoint(*env, &wasm_name) {
            let path = cjs_dir.join(format!("{}.cjs", env.file_stem()));
            std::fs::write(&path, content)?;
        }
    }

    // Bundle entrypoints that need it (IIFE and CJS versions of ESM-only targets)
    println!("  Bundling with esbuild...");
    bundle_with_esbuild(out_dir, crate_name)?;

    Ok(())
}

fn bundle_with_esbuild(out_dir: &Path, crate_name: &str) -> Result<()> {
    let esbuild = find_esbuild()?;

    // Bundle IIFE from web entrypoint
    let esm_web = out_dir.join(targets::paths::esm_entrypoint(Environment::Web));
    let iife_output = out_dir.join(targets::paths::iife_bundle());
    let global_name = crate_name.to_pascal_case();

    run_esbuild(&esbuild, &esm_web, &iife_output, "iife", Some(&global_name))?;

    // Bundle CJS versions for environments that need it
    for env in Environment::all() {
        if env.needs_cjs_bundle() {
            let esm_path = out_dir.join(targets::paths::esm_entrypoint(*env));
            let cjs_path = out_dir.join(targets::paths::cjs_entrypoint(*env));
            run_esbuild(&esbuild, &esm_path, &cjs_path, "cjs", None)?;
        }
    }

    Ok(())
}

fn run_esbuild(
    esbuild: &str,
    input: &Path,
    output: &Path,
    format: &str,
    global_name: Option<&str>,
) -> Result<()> {
    let mut args = vec![
        input.to_str().unwrap().to_string(),
        "--bundle".to_string(),
        format!("--format={}", format),
        format!("--outfile={}", output.display()),
        // Suppress warning about import.meta in non-ESM formats - we don't use that code path
        "--log-override:empty-import-meta=silent".to_string(),
    ];

    if format == "cjs" {
        args.push("--platform=node".to_string());
    }

    if let Some(name) = global_name {
        args.push(format!("--global-name={}", name));
    }

    let status = Command::new(esbuild)
        .args(&args)
        .status()
        .with_context(|| format!("Failed to run esbuild for {} bundle", format))?;

    if !status.success() {
        anyhow::bail!("esbuild {} bundle failed", format);
    }

    Ok(())
}

fn find_esbuild() -> Result<String> {
    // Try common locations
    let candidates = [
        "esbuild",                      // System PATH
        "./node_modules/.bin/esbuild",  // Local node_modules
        "../node_modules/.bin/esbuild", // Parent node_modules
    ];

    for candidate in candidates {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(candidate.to_string());
        }
    }

    anyhow::bail!(
        "esbuild not found. Please install it:\n  \
         npm install -g esbuild\n  \
         or: npm install --save-dev esbuild"
    )
}
