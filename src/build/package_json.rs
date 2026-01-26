use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;

use super::targets::{self, Environment, ExportCondition, ROOT_EXPORT_MAPPING};

/// Update package.json with generated fields and exports map.
pub fn update(package_json_path: &Path, out_dir_rel: &Path, package_name: &str) -> Result<()> {
    let dist = out_dir_rel.display().to_string();

    // Read existing package.json
    let package_content =
        std::fs::read_to_string(package_json_path).context("Failed to read package.json")?;
    let mut package: Value =
        serde_json::from_str(&package_content).context("Failed to parse package.json")?;

    let package_obj = package
        .as_object_mut()
        .context("package.json must be an object")?;

    // Set standard fields
    package_obj.insert("type".to_string(), json!("module"));
    package_obj.insert(
        "main".to_string(),
        json!(format!(
            "./{}/{}",
            dist,
            targets::paths::cjs_entrypoint(Environment::Node).display()
        )),
    );
    package_obj.insert(
        "module".to_string(),
        json!(format!(
            "./{}/{}",
            dist,
            targets::paths::esm_entrypoint(Environment::Bundler).display()
        )),
    );
    package_obj.insert(
        "types".to_string(),
        json!(format!("./{}/{}", dist, targets::paths::types().display())),
    );

    // Update files array to include out_dir
    update_files_array(package_obj, &dist);

    // Generate exports map
    let exports = build_exports_map(&dist, package_name);
    package_obj.insert("exports".to_string(), exports);

    // Write updated package.json
    let output_content = serde_json::to_string_pretty(&package)?;
    std::fs::write(package_json_path, output_content)?;
    println!("  Updated package.json");

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
fn build_exports_map(dist: &str, package_name: &str) -> Value {
    // Helper to format a path with the dist prefix
    let p = |path: &Path| format!("./{}/{}", dist, path.display());

    // Build the root "." export with conditional resolution
    let mut root_export = serde_json::Map::new();

    // Types first (for TypeScript)
    root_export.insert("types".to_string(), json!(p(&targets::paths::types())));

    // Add each condition from the mapping
    for mapping in ROOT_EXPORT_MAPPING {
        let esm_path = p(&targets::paths::esm_entrypoint(mapping.esm));
        let cjs_path = p(&targets::paths::cjs_entrypoint(mapping.cjs));

        match mapping.condition {
            ExportCondition::Import => {
                root_export.insert("import".to_string(), json!(esm_path));
            }
            ExportCondition::Require => {
                root_export.insert("require".to_string(), json!(cjs_path));
            }
            _ => {
                // Nested condition with import/require
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

    json!({
        ".": root_export,
        "./slim": {
            "types": p(&targets::paths::types()),
            "import": p(&targets::paths::esm_entrypoint(Environment::Slim)),
            "require": p(&targets::paths::cjs_entrypoint(Environment::Slim))
        },
        "./wasm": p(&targets::paths::standalone_wasm(package_name)),
        "./wasm-base64": {
            "import": p(&targets::paths::wasm_base64_esm()),
            "require": p(&targets::paths::wasm_base64_cjs())
        },
        "./iife": p(&targets::paths::iife_bundle())
    })
}
