use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;

use super::targets::{self, Environment, ExportCondition, ROOT_EXPORT_MAPPING, WasmVariant};

/// Update package.json with generated fields and exports map.
pub fn update(
    package_json_path: &Path,
    out_dir_rel: &Path,
    package_name: &str,
    available_variants: &[WasmVariant],
) -> Result<()> {
    let dist = out_dir_rel.display().to_string();
    let has_debug = available_variants.contains(&WasmVariant::Debug);

    // Read existing package.json
    let package_content =
        std::fs::read_to_string(package_json_path).context("Failed to read package.json")?;
    let mut package: Value =
        serde_json::from_str(&package_content).context("Failed to parse package.json")?;

    let package_obj = package
        .as_object_mut()
        .context("package.json must be an object")?;

    // Set standard fields -- these always point to the optimized variant.
    package_obj.insert("type".to_string(), json!("module"));
    package_obj.insert(
        "main".to_string(),
        json!(format!(
            "./{}/{}",
            dist,
            targets::paths::cjs_entrypoint(Environment::Node, WasmVariant::Optimized).display()
        )),
    );
    package_obj.insert(
        "module".to_string(),
        json!(format!(
            "./{}/{}",
            dist,
            targets::paths::esm_entrypoint(Environment::Bundler, WasmVariant::Optimized).display()
        )),
    );
    package_obj.insert(
        "types".to_string(),
        json!(format!("./{}/{}", dist, targets::paths::types().display())),
    );

    update_side_effects(package_obj, &dist, has_debug)?;

    // Update files array to include out_dir
    update_files_array(package_obj, &dist);

    // Generate exports map
    let exports = build_exports_map(&dist, package_name, has_debug);
    package_obj.insert("exports".to_string(), exports);

    // Write updated package.json
    let output_content = serde_json::to_string_pretty(&package)?;
    std::fs::write(package_json_path, output_content)?;
    println!("  Updated package.json");

    Ok(())
}

fn update_side_effects(
    package_obj: &mut serde_json::Map<String, Value>,
    dist: &str,
    has_debug: bool,
) -> Result<()> {
    let side_effects = package_obj
        .entry("sideEffects")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    let serde_json::Value::Array(actual_effects) = side_effects else {
        anyhow::bail!("sideEffects key of package.json was not an array");
    };
    let mut required_effects = vec![
        format!("./{}/esm/bundler.js", dist),
        format!("./{}/esm/node.js", dist),
        format!("./{}/esm/web.js", dist),
        format!("./{}/esm/workerd.js", dist),
    ];
    if has_debug {
        required_effects.extend([
            format!("./{}/esm/debug-bundler.js", dist),
            format!("./{}/esm/debug-node.js", dist),
            format!("./{}/esm/debug-web.js", dist),
            format!("./{}/esm/debug-workerd.js", dist),
        ]);
    }
    for effect in required_effects {
        let effect = serde_json::Value::String(effect.to_string());
        if !actual_effects.contains(&effect) {
            actual_effects.push(effect);
        }
    }
    Ok(())
}

fn update_files_array(package_obj: &mut serde_json::Map<String, Value>, dist: &str) {
    let mut files: Vec<String> = package_obj
        .get("files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Add dist dir if not already present
    if !files
        .iter()
        .any(|f| f == dist || f.starts_with(&format!("{}/", dist)))
    {
        files.push(dist.to_string());
    }
    package_obj.insert("files".to_string(), json!(files));
}

/// Build the exports map for package.json based on the declarative mapping in targets.rs
fn build_exports_map(dist: &str, package_name: &str, has_debug: bool) -> Value {
    let p = |path: &Path| format!("./{}/{}", dist, path.display());

    let mut exports = serde_json::Map::new();

    // Root "." + ./slim + ./wasm + ./wasm-base64 + ./iife use optimized variant
    exports.insert(
        ".".to_string(),
        build_conditional_export(dist, WasmVariant::Optimized),
    );
    exports.insert(
        "./slim".to_string(),
        json!({
            "types": p(&targets::paths::types()),
            "import": p(&targets::paths::esm_entrypoint(Environment::Slim, WasmVariant::Optimized)),
            "require": p(&targets::paths::cjs_entrypoint(Environment::Slim, WasmVariant::Optimized))
        }),
    );
    exports.insert(
        "./wasm".to_string(),
        json!(p(&targets::paths::standalone_wasm(
            package_name,
            WasmVariant::Optimized
        ))),
    );
    exports.insert(
        "./wasm-base64".to_string(),
        json!({
            "import": p(&targets::paths::wasm_base64_esm(WasmVariant::Optimized)),
            "require": p(&targets::paths::wasm_base64_cjs(WasmVariant::Optimized))
        }),
    );
    exports.insert(
        "./iife".to_string(),
        json!(p(&targets::paths::iife_bundle(WasmVariant::Optimized))),
    );

    // Debug variant exports: mirror ./, ./slim, ./wasm, ./wasm-base64, ./iife.
    //
    // `./debug/slim` is load-bearing, not just ergonomic: `wasm-opt`
    // renames wasm exports in the optimized variant (see
    // `cjs_web_bindings` in targets.rs), so the JS bindings re-exported by
    // `./slim` are pinned to the optimized wasm's renamed symbol names. A
    // consumer who pairs `./slim` with `./debug/wasm` hits a runtime
    // `TypeError: wasm.__wbindgen_export3 is not a function` during the
    // first call into the module. `./debug/slim` re-exports the debug
    // variant's wasm-bindgen JS and must be used alongside `./debug/wasm`
    // (or `./debug/wasm-base64`) as a matched pair.
    if has_debug {
        exports.insert(
            "./debug".to_string(),
            build_conditional_export(dist, WasmVariant::Debug),
        );
        exports.insert(
            "./debug/slim".to_string(),
            json!({
                "types": p(&targets::paths::types()),
                "import": p(&targets::paths::esm_entrypoint(Environment::Slim, WasmVariant::Debug)),
                "require": p(&targets::paths::cjs_entrypoint(Environment::Slim, WasmVariant::Debug))
            }),
        );
        exports.insert(
            "./debug/wasm".to_string(),
            json!(p(&targets::paths::standalone_wasm(
                package_name,
                WasmVariant::Debug
            ))),
        );
        exports.insert(
            "./debug/wasm-base64".to_string(),
            json!({
                "import": p(&targets::paths::wasm_base64_esm(WasmVariant::Debug)),
                "require": p(&targets::paths::wasm_base64_cjs(WasmVariant::Debug))
            }),
        );
        exports.insert(
            "./debug/iife".to_string(),
            json!(p(&targets::paths::iife_bundle(WasmVariant::Debug))),
        );
    }

    Value::Object(exports)
}

/// Build the conditional export object for either `.` or `./debug`. Has
/// identical shape (types + conditions), differing only in which variant's
/// entrypoint files it points at.
fn build_conditional_export(dist: &str, variant: WasmVariant) -> Value {
    let p = |path: &Path| format!("./{}/{}", dist, path.display());

    let mut root_export = serde_json::Map::new();
    root_export.insert("types".to_string(), json!(p(&targets::paths::types())));

    for mapping in ROOT_EXPORT_MAPPING {
        let esm_path = p(&targets::paths::esm_entrypoint(mapping.esm, variant));
        let cjs_path = p(&targets::paths::cjs_entrypoint(mapping.cjs, variant));

        match mapping.condition {
            ExportCondition::Import => {
                root_export.insert("import".to_string(), json!(esm_path));
            }
            ExportCondition::Require => {
                root_export.insert("require".to_string(), json!(cjs_path));
            }
            _ => {
                root_export.insert(
                    mapping.condition.as_str().to_string(),
                    json!({
                        "import": esm_path,
                        "require": cjs_path
                    }),
                );
            }
        }
    }

    Value::Object(root_export)
}
