use anyhow::{Context, Result};
use heck::ToPascalCase;
use std::path::Path;
use std::process::Command;

use super::targets::{self, Environment, WasmVariant};

/// Generate all entrypoints (ESM, CJS, IIFE) for every variant that was built.
pub fn generate(out_dir: &Path, crate_name: &str) -> Result<()> {
    let wasm_name = crate_name.replace('-', "_");
    let esm_dir = out_dir.join("esm");
    let cjs_dir = out_dir.join("cjs");
    let iife_dir = out_dir.join("iife");

    std::fs::create_dir_all(&esm_dir)?;
    std::fs::create_dir_all(&cjs_dir)?;
    std::fs::create_dir_all(&iife_dir)?;

    for variant in WasmVariant::all() {
        // Skip variants whose wasm-bindgen output isn't present (e.g. a
        // --wasm-bindgen-tar tarball that only contains the optimized dirs).
        let web_dir = out_dir.join(format!("wasm_bindgen/web{}", variant.dir_suffix()));
        if !web_dir.exists() {
            continue;
        }

        println!("  Generating ESM entrypoints ({})...", variant,);
        for env in Environment::all() {
            let content = targets::generate_esm_entrypoint(*env, &wasm_name, *variant);
            let path = out_dir.join(targets::paths::esm_entrypoint(*env, *variant));
            std::fs::write(&path, content)?;
        }

        println!("  Generating CJS entrypoints ({})...", variant,);
        for env in Environment::all() {
            if let Some(content) = targets::generate_cjs_entrypoint(*env, &wasm_name, *variant) {
                let path = out_dir.join(targets::paths::cjs_entrypoint(*env, *variant));
                std::fs::write(&path, content)?;
            }
        }
    }

    // Bundle entrypoints that need it (IIFE and CJS versions of ESM-only targets)
    println!("  Bundling with esbuild...");
    bundle_with_esbuild(out_dir, crate_name)?;

    Ok(())
}

fn bundle_with_esbuild(out_dir: &Path, crate_name: &str) -> Result<()> {
    let esbuild = find_esbuild()?;
    let wasm_name = crate_name.replace('-', "_");
    let global_name = crate_name.to_pascal_case();

    // Per-variant bundles: web-bindings.cjs, IIFE, and CJS-for-ESM-envs.
    // Each variant has its own web-bindings.cjs because wasm-opt renames wasm
    // exports in the optimized variant, causing the wasm-bindgen JS to diverge
    // between variants.
    for variant in WasmVariant::all() {
        let web_dir = out_dir.join(format!("wasm_bindgen/web{}", variant.dir_suffix()));
        if !web_dir.exists() {
            continue;
        }

        // Bundle this variant's web-bindings.cjs from its own wasm-bindgen JS.
        let web_js = web_dir.join(format!("{}.js", wasm_name));
        let web_bindings_cjs = out_dir.join(targets::paths::cjs_web_bindings(*variant));
        run_esbuild(&esbuild, &web_js, &web_bindings_cjs, "cjs", None)?;

        // Bundle IIFE from this variant's web entrypoint
        let esm_web = out_dir.join(targets::paths::esm_entrypoint(Environment::Web, *variant));
        let iife_output = out_dir.join(targets::paths::iife_bundle(*variant));
        let iife_global = if variant.is_debug() {
            format!("{}Debug", global_name)
        } else {
            global_name.clone()
        };
        run_esbuild(&esbuild, &esm_web, &iife_output, "iife", Some(&iife_global))?;

        // Bundle CJS versions for environments that need it
        for env in Environment::all() {
            if env.needs_cjs_bundle() {
                let esm_path = out_dir.join(targets::paths::esm_entrypoint(*env, *variant));
                let cjs_path = out_dir.join(targets::paths::cjs_entrypoint(*env, *variant));
                run_esbuild(&esbuild, &esm_path, &cjs_path, "cjs", None)?;
            }
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
