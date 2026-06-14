use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;

use super::targets::{self, Environment, ExportCondition, ROOT_EXPORT_MAPPING, WasmVariant};
use super::wrapper::BuiltWrapper;

/// Update package.json with generated fields and exports map.
pub fn update(
    package_json_path: &Path,
    out_dir_rel: &Path,
    package_name: &str,
    available_variants: &[WasmVariant],
    wrapper: Option<&BuiltWrapper>,
) -> Result<()> {
    let dist = out_dir_rel.display().to_string();
    let has_debug = available_variants.contains(&WasmVariant::Debug);
    let has_wrapper = wrapper.is_some();

    // Read existing package.json
    let package_content =
        std::fs::read_to_string(package_json_path).context("Failed to read package.json")?;
    let mut package: Value =
        serde_json::from_str(&package_content).context("Failed to parse package.json")?;

    let package_obj = package
        .as_object_mut()
        .context("package.json must be an object")?;

    // Set standard fields -- these always point to the optimized variant. In
    // wrapper mode they point at the handwritten TypeScript wrapper outputs;
    // otherwise they point at the standard raw wasm-bindgen API.
    package_obj.insert("type".to_string(), json!("module"));
    package_obj.insert(
        "main".to_string(),
        json!(format!(
            "./{}/{}",
            dist,
            cjs_entrypoint_path(Environment::Node, WasmVariant::Optimized, has_wrapper).display()
        )),
    );
    package_obj.insert(
        "module".to_string(),
        json!(format!(
            "./{}/{}",
            dist,
            esm_entrypoint_path(Environment::Bundler, WasmVariant::Optimized, has_wrapper)
                .display()
        )),
    );
    package_obj.insert(
        "types".to_string(),
        json!(format!("./{}/{}", dist, types_path(has_wrapper).display())),
    );

    update_side_effects(package_obj, &dist, has_debug, has_wrapper)?;

    // Update files array to include out_dir
    update_files_array(package_obj, &dist);

    // Generate exports map
    let exports = build_exports_map(&dist, package_name, has_debug, wrapper);
    package_obj.insert("exports".to_string(), exports);

    if let Some(wrapper) = wrapper {
        update_package_imports(package_obj, &dist, wrapper, has_debug)?;
    }

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
    has_wrapper: bool,
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
    if has_wrapper {
        required_effects.extend([
            format!("./{}/wrapper/esm/bundler.js", dist),
            format!("./{}/wrapper/esm/node.js", dist),
            format!("./{}/wrapper/esm/web.js", dist),
            format!("./{}/wrapper/esm/workerd.js", dist),
        ]);
        if has_debug {
            required_effects.extend([
                format!("./{}/wrapper/esm/debug-bundler.js", dist),
                format!("./{}/wrapper/esm/debug-node.js", dist),
                format!("./{}/wrapper/esm/debug-web.js", dist),
                format!("./{}/wrapper/esm/debug-workerd.js", dist),
            ]);
        }
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

/// Build the exports map for package.json based on the declarative mapping in targets.rs.
fn build_exports_map(
    dist: &str,
    package_name: &str,
    has_debug: bool,
    wrapper: Option<&BuiltWrapper>,
) -> Value {
    let p = |path: &Path| format!("./{}/{}", dist, path.display());
    let has_wrapper = wrapper.is_some();
    let has_wrapper_slim = wrapper.is_some_and(|w| w.has_slim);
    let raw_slim_types = wrapper.map(|w| w.raw_slim_types.as_path());

    let mut exports = serde_json::Map::new();

    // Root "." + ./slim + ./wasm + ./wasm-base64 + ./iife use optimized variant.
    // In wrapper mode only the ergonomic JS entrypoints move; raw wasm assets
    // keep their historical locations.
    exports.insert(
        ".".to_string(),
        build_conditional_export(dist, WasmVariant::Optimized, has_wrapper),
    );
    exports.insert(
        "./slim".to_string(),
        build_slim_export(
            dist,
            WasmVariant::Optimized,
            has_wrapper_slim,
            raw_slim_types,
        ),
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
        json!(p(&iife_bundle_path(WasmVariant::Optimized, has_wrapper))),
    );

    if wrapper.is_some_and(|w| w.expose_bindings) {
        exports.insert(
            "./bindings".to_string(),
            build_conditional_export(dist, WasmVariant::Optimized, false),
        );
        exports.insert(
            "./bindings/slim".to_string(),
            build_slim_export(dist, WasmVariant::Optimized, false, raw_slim_types),
        );
    }

    // Debug variant exports mirror the optimized side: ./, ./slim, ./wasm,
    // ./wasm-base64, ./iife. ./debug/slim exists because the debug wasm has
    // different imports than the optimized wasm (e.g. __wbindgen_throw is
    // optimized away in release), so the JS bindings paired with each variant
    // are not interchangeable.
    if has_debug {
        exports.insert(
            "./debug".to_string(),
            build_conditional_export(dist, WasmVariant::Debug, has_wrapper),
        );
        exports.insert(
            "./debug/slim".to_string(),
            build_slim_export(dist, WasmVariant::Debug, has_wrapper_slim, raw_slim_types),
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
            json!(p(&iife_bundle_path(WasmVariant::Debug, has_wrapper))),
        );

        if wrapper.is_some_and(|w| w.expose_bindings) {
            exports.insert(
                "./bindings/debug".to_string(),
                build_conditional_export(dist, WasmVariant::Debug, false),
            );
            exports.insert(
                "./bindings/debug/slim".to_string(),
                build_slim_export(dist, WasmVariant::Debug, false, raw_slim_types),
            );
        }
    }

    Value::Object(exports)
}

/// Build the conditional export object for either `.` or `./debug`. Has
/// identical shape (types + conditions), differing only in which variant's
/// entrypoint files it points at and whether those files are raw bindings or
/// wrapper outputs.
fn build_conditional_export(dist: &str, variant: WasmVariant, wrapper: bool) -> Value {
    let p = |path: &Path| format!("./{}/{}", dist, path.display());

    let mut root_export = serde_json::Map::new();
    root_export.insert("types".to_string(), json!(p(&types_path(wrapper))));

    for mapping in ROOT_EXPORT_MAPPING {
        let esm_path = p(&esm_entrypoint_path(mapping.esm, variant, wrapper));
        let cjs_path = p(&cjs_entrypoint_path(mapping.cjs, variant, wrapper));

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

fn build_slim_export(
    dist: &str,
    variant: WasmVariant,
    wrapper: bool,
    raw_slim_types: Option<&Path>,
) -> Value {
    let p = |path: &Path| format!("./{}/{}", dist, path.display());
    let types = if wrapper {
        targets::paths::wrapper_slim_types()
    } else {
        raw_slim_types
            .map(Path::to_path_buf)
            .unwrap_or_else(targets::paths::types)
    };
    json!({
        "types": p(&types),
        "import": p(&esm_entrypoint_path(Environment::Slim, variant, wrapper)),
        "require": p(&cjs_entrypoint_path(Environment::Slim, variant, wrapper))
    })
}

fn update_package_imports(
    package_obj: &mut serde_json::Map<String, Value>,
    dist: &str,
    wrapper: &BuiltWrapper,
    has_debug: bool,
) -> Result<()> {
    let imports = package_obj
        .entry("imports")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let serde_json::Value::Object(imports_obj) = imports else {
        anyhow::bail!("imports key of package.json was not an object");
    };

    imports_obj.insert(
        "#wasm-bodge/bindings".to_string(),
        build_conditional_export(dist, WasmVariant::Optimized, false),
    );
    imports_obj.insert(
        "#wasm-bodge/bindings/slim".to_string(),
        build_slim_export(
            dist,
            WasmVariant::Optimized,
            false,
            Some(&wrapper.raw_slim_types),
        ),
    );

    if has_debug {
        imports_obj.insert(
            "#wasm-bodge/bindings/debug".to_string(),
            build_conditional_export(dist, WasmVariant::Debug, false),
        );
        imports_obj.insert(
            "#wasm-bodge/bindings/debug/slim".to_string(),
            build_slim_export(
                dist,
                WasmVariant::Debug,
                false,
                Some(&wrapper.raw_slim_types),
            ),
        );
    }

    Ok(())
}

fn types_path(wrapper: bool) -> std::path::PathBuf {
    if wrapper {
        targets::paths::wrapper_types()
    } else {
        targets::paths::types()
    }
}

fn esm_entrypoint_path(
    env: Environment,
    variant: WasmVariant,
    wrapper: bool,
) -> std::path::PathBuf {
    if wrapper {
        targets::paths::wrapper_esm_entrypoint(env, variant)
    } else {
        targets::paths::esm_entrypoint(env, variant)
    }
}

fn cjs_entrypoint_path(
    env: Environment,
    variant: WasmVariant,
    wrapper: bool,
) -> std::path::PathBuf {
    if wrapper {
        targets::paths::wrapper_cjs_entrypoint(env, variant)
    } else {
        targets::paths::cjs_entrypoint(env, variant)
    }
}

fn iife_bundle_path(variant: WasmVariant, wrapper: bool) -> std::path::PathBuf {
    if wrapper {
        targets::paths::wrapper_iife_bundle(variant)
    } else {
        targets::paths::iife_bundle(variant)
    }
}
