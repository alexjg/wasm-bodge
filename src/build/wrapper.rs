use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::WrapperConfig;

use super::targets::{self, Environment, WasmBindgenTarget, WasmVariant};

const BINDINGS_SPECIFIER: &str = "#wasm-bodge/bindings";
const SLIM_BINDINGS_SPECIFIER: &str = "#wasm-bodge/bindings/slim";
const TSCONFIG_JSON: &str = "tsconfig.json";

/// Information about wrapper outputs that package.json generation needs.
#[derive(Debug, Clone)]
pub struct BuiltWrapper {
    pub has_slim: bool,
    pub expose_bindings: bool,
    /// Path to the raw web-target declarations, relative to out_dir. These
    /// include manual-initialization exports like initSync and are therefore
    /// the right types for slim bindings.
    pub raw_slim_types: PathBuf,
}

/// Read optional wrapper configuration from package.json.
pub fn read_config(package_json_path: &Path) -> Result<Option<WrapperConfig>> {
    let package_content =
        std::fs::read_to_string(package_json_path).context("Failed to read package.json")?;
    let package: Value =
        serde_json::from_str(&package_content).context("Failed to parse package.json")?;

    let Some(wrapper) = package
        .get("wasm-bodge")
        .and_then(|config| config.get("wrapper"))
    else {
        return Ok(None);
    };

    if wrapper.is_null() || wrapper == &Value::Bool(false) {
        return Ok(None);
    }

    let wrapper_obj = wrapper
        .as_object()
        .context("package.json wasm-bodge.wrapper must be an object")?;

    let entry = wrapper_obj
        .get("entry")
        .and_then(Value::as_str)
        .context("package.json wasm-bodge.wrapper.entry must be a string")?;

    let slim_entry = wrapper_obj
        .get("slimEntry")
        .or_else(|| wrapper_obj.get("slim_entry"))
        .and_then(Value::as_str)
        .map(PathBuf::from);

    let tsconfig = wrapper_obj
        .get("tsconfig")
        .or_else(|| wrapper_obj.get("tsConfig"))
        .and_then(Value::as_str)
        .map(PathBuf::from);

    let expose_bindings = wrapper_obj
        .get("exposeBindings")
        .or_else(|| wrapper_obj.get("expose_bindings"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let source_map = wrapper_obj
        .get("sourceMap")
        .or_else(|| wrapper_obj.get("source_map"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let externals = match wrapper_obj.get("externals") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(entries)) => entries
            .iter()
            .map(|entry| {
                let spec = entry.as_str().context(
                    "package.json wasm-bodge.wrapper.externals must be an array of strings",
                )?;
                if spec.contains('*') {
                    anyhow::bail!(
                        "package.json wasm-bodge.wrapper.externals does not support wildcards; \
                         list each specifier exactly, e.g. \"@automerge/automerge/next\""
                    );
                }
                Ok(spec.to_string())
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => {
            anyhow::bail!("package.json wasm-bodge.wrapper.externals must be an array of strings")
        }
    };

    Ok(Some(WrapperConfig {
        entry: PathBuf::from(entry),
        slim_entry,
        tsconfig,
        expose_bindings,
        source_map,
        externals,
    }))
}

/// Build handwritten TypeScript wrappers for each runtime-specific raw
/// wasm-bodge entrypoint. The raw files stay in their historical locations;
/// wrapper outputs live under dist/wrapper/ and package.json points the root
/// export at them.
pub fn build(
    package_json_path: &Path,
    out_dir: &Path,
    wasm_name: &str,
    config: &WrapperConfig,
    available_variants: &[WasmVariant],
) -> Result<BuiltWrapper> {
    println!("  Building TypeScript wrapper...");

    let package_dir = package_json_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .context("Failed to canonicalize package.json directory")?;
    let entry = resolve_package_path(&package_dir, &config.entry)?;
    ensure_file_exists(&entry, "wasm-bodge.wrapper.entry")?;

    let slim_entry = config
        .slim_entry
        .as_ref()
        .map(|path| resolve_package_path(&package_dir, path))
        .transpose()?;
    if let Some(path) = slim_entry.as_deref() {
        ensure_file_exists(path, "wasm-bodge.wrapper.slimEntry")?;
    }
    let effective_slim_entry = slim_entry.as_deref().unwrap_or(&entry);

    let tsconfig = resolve_tsconfig(&package_dir, &entry, config.tsconfig.as_deref())?;

    for variant in available_variants {
        build_root_wrapper_variant(
            out_dir,
            &entry,
            *variant,
            config.source_map,
            tsconfig.as_deref(),
            &config.externals,
        )?;
        build_slim_wrapper_variant(
            out_dir,
            effective_slim_entry,
            *variant,
            config.source_map,
            tsconfig.as_deref(),
            &config.externals,
        )?;
    }

    let raw_slim_types =
        targets::paths::wasm_bindgen_dir(WasmBindgenTarget::Web).join(format!("{wasm_name}.d.ts"));

    emit_declarations(
        &package_dir,
        out_dir,
        &entry,
        effective_slim_entry,
        &raw_slim_types,
        tsconfig.as_deref(),
    )?;
    write_dev_helper(&package_dir, out_dir, &raw_slim_types)?;

    Ok(BuiltWrapper {
        has_slim: true,
        expose_bindings: config.expose_bindings,
        raw_slim_types,
    })
}

fn resolve_package_path(package_dir: &Path, path: &Path) -> Result<PathBuf> {
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        package_dir.join(path)
    };
    Ok(resolved)
}

fn ensure_file_exists(path: &Path, label: &str) -> Result<()> {
    if !path.is_file() {
        anyhow::bail!("{label} does not point to a file: {}", path.display());
    }
    Ok(())
}

fn resolve_tsconfig(
    package_dir: &Path,
    entry: &Path,
    configured: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if let Some(configured) = configured {
        let tsconfig = resolve_package_path(package_dir, configured)?;
        ensure_file_exists(&tsconfig, "wasm-bodge.wrapper.tsconfig")?;
        return Ok(Some(tsconfig));
    }

    // Prefer the nearest tsconfig at or above the wrapper entry, stopping at
    // the package directory. This matches the file esbuild would normally find
    // when run from the package root and avoids silently ignoring normal TS
    // project settings like path aliases and JSX options.
    let mut current = entry.parent();
    while let Some(dir) = current {
        let candidate = dir.join(TSCONFIG_JSON);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        if dir == package_dir {
            break;
        }
        current = dir.parent();
    }

    let candidate = package_dir.join(TSCONFIG_JSON);
    if candidate.is_file() {
        return Ok(Some(candidate));
    }

    Ok(None)
}

fn build_root_wrapper_variant(
    out_dir: &Path,
    entry: &Path,
    variant: WasmVariant,
    source_map: bool,
    tsconfig: Option<&Path>,
    externals: &[String],
) -> Result<()> {
    println!("  Building wrapper entrypoints ({variant})...");

    for env in [
        Environment::Node,
        Environment::Web,
        Environment::Bundler,
        Environment::Workerd,
    ] {
        let output = out_dir.join(targets::paths::wrapper_esm_entrypoint(env, variant));
        bundle_wrapper_entry(
            entry,
            &output,
            "esm",
            None,
            source_map,
            tsconfig,
            &raw_esm_specifier(env, variant),
            &raw_esm_specifier(Environment::Slim, variant),
            externals,
        )?;
    }

    for env in [Environment::Node, Environment::Web] {
        let output = out_dir.join(targets::paths::wrapper_cjs_entrypoint(env, variant));
        bundle_wrapper_entry(
            entry,
            &output,
            "cjs",
            None,
            source_map,
            tsconfig,
            &raw_cjs_specifier(env, variant),
            &raw_cjs_specifier(Environment::Slim, variant),
            externals,
        )?;
    }

    let web_wrapper = out_dir.join(targets::paths::wrapper_esm_entrypoint(
        Environment::Web,
        variant,
    ));
    let iife_output = out_dir.join(targets::paths::wrapper_iife_bundle(variant));
    bundle_iife(&web_wrapper, &iife_output, variant, source_map, externals)?;

    Ok(())
}

fn build_slim_wrapper_variant(
    out_dir: &Path,
    entry: &Path,
    variant: WasmVariant,
    source_map: bool,
    tsconfig: Option<&Path>,
    externals: &[String],
) -> Result<()> {
    println!("  Building slim wrapper entrypoints ({variant})...");

    let output = out_dir.join(targets::paths::wrapper_esm_entrypoint(
        Environment::Slim,
        variant,
    ));
    let raw_esm_slim = raw_esm_specifier(Environment::Slim, variant);
    bundle_wrapper_entry(
        entry,
        &output,
        "esm",
        None,
        source_map,
        tsconfig,
        &raw_esm_slim,
        &raw_esm_slim,
        externals,
    )?;

    let output = out_dir.join(targets::paths::wrapper_cjs_entrypoint(
        Environment::Slim,
        variant,
    ));
    let raw_cjs_slim = raw_cjs_specifier(Environment::Slim, variant);
    bundle_wrapper_entry(
        entry,
        &output,
        "cjs",
        None,
        source_map,
        tsconfig,
        &raw_cjs_slim,
        &raw_cjs_slim,
        externals,
    )?;

    Ok(())
}

fn raw_esm_specifier(env: Environment, variant: WasmVariant) -> String {
    format!(
        "../../{}",
        targets::paths::esm_entrypoint(env, variant).display()
    )
}

fn raw_cjs_specifier(env: Environment, variant: WasmVariant) -> String {
    format!(
        "../../{}",
        targets::paths::cjs_entrypoint(env, variant).display()
    )
}

#[allow(clippy::too_many_arguments)]
fn bundle_wrapper_entry(
    entry: &Path,
    output: &Path,
    format: &str,
    global_name: Option<&str>,
    source_map: bool,
    tsconfig: Option<&Path>,
    bindings_specifier: &str,
    slim_bindings_specifier: &str,
    externals: &[String],
) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let esbuild = find_esbuild()?;
    let mut args = vec![
        entry.display().to_string(),
        "--bundle".to_string(),
        format!("--format={format}"),
        "--target=es2022".to_string(),
        format!("--outfile={}", output.display()),
        format!("--external:{BINDINGS_SPECIFIER}"),
        format!("--external:{SLIM_BINDINGS_SPECIFIER}"),
        // Suppress warning about import.meta in non-ESM formats - wrapper code
        // may import raw entrypoints that contain guarded import.meta paths.
        "--log-override:empty-import-meta=silent".to_string(),
    ];

    for external in externals {
        args.push(format!("--external:{external}"));
    }

    if format == "cjs" {
        args.push("--platform=node".to_string());
    }
    if source_map {
        args.push("--sourcemap".to_string());
    }
    if let Some(tsconfig) = tsconfig {
        args.push(format!("--tsconfig={}", tsconfig.display()));
    }
    if let Some(global_name) = global_name {
        args.push(format!("--global-name={global_name}"));
    }

    let status = Command::new(&esbuild)
        .args(&args)
        .status()
        .with_context(|| format!("Failed to run esbuild for wrapper {}", output.display()))?;

    if !status.success() {
        anyhow::bail!("esbuild wrapper bundle failed for {}", output.display());
    }

    rewrite_virtual_imports(output, bindings_specifier, slim_bindings_specifier)?;
    Ok(())
}

fn bundle_iife(
    input: &Path,
    output: &Path,
    variant: WasmVariant,
    source_map: bool,
    externals: &[String],
) -> Result<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let esbuild = find_esbuild()?;
    let global_name = if variant.is_debug() {
        "WasmBodgeWrapperDebug"
    } else {
        "WasmBodgeWrapper"
    };

    // The wrapper IIFE is always wrapped in a factory function so the
    // interface is the same whether or not externals are configured. An IIFE
    // has no module resolution at runtime, so each configured external is
    // aliased to a shim that reads the dependency from a store, and the
    // factory populates the store before lazily requiring the wrapper:
    //
    // ```js
    // const api = WasmBodgeWrapper({ "@automerge/automerge": Automerge });
    // const standalone = WasmBodgeWrapper();
    // ```
    let factory = prepare_iife_factory_entry(input, externals)?;

    let mut args = vec![
        factory.entry.display().to_string(),
        "--bundle".to_string(),
        "--format=iife".to_string(),
        "--target=es2022".to_string(),
        format!("--global-name={global_name}"),
        format!("--outfile={}", output.display()),
        "--log-override:empty-import-meta=silent".to_string(),
    ];
    for (spec, shim) in &factory.aliases {
        args.push(format!("--alias:{spec}={}", shim.display()));
    }
    if source_map {
        args.push("--sourcemap".to_string());
    }

    let status = Command::new(&esbuild)
        .args(&args)
        .status()
        .with_context(|| {
            format!(
                "Failed to run esbuild for wrapper IIFE {}",
                output.display()
            )
        })?;

    factory.cleanup();

    if !status.success() {
        anyhow::bail!(
            "esbuild wrapper IIFE bundle failed for {}",
            output.display()
        );
    }

    Ok(())
}

/// Generated files for a factory-style wrapper IIFE. See
/// `prepare_iife_factory_entry`.
struct IifeFactoryEntry {
    /// The generated entrypoint to bundle.
    entry: PathBuf,
    /// External specifier -> shim file to pass to esbuild as `--alias`.
    aliases: Vec<(String, PathBuf)>,
    /// All generated files, removed after bundling.
    generated: Vec<PathBuf>,
}

impl IifeFactoryEntry {
    fn cleanup(&self) {
        for path in &self.generated {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Generate the factory entrypoint for a wrapper IIFE.
///
/// The entry is a tiny CJS module placed next to `input` (the web wrapper
/// entrypoint) whose `require("./web.js")` esbuild keeps lazy, so wrapper
/// evaluation — including wasm initialization — is deferred until the caller
/// invokes the factory.
///
/// Each configured external gets a CJS shim that reads the dependency from a
/// store module populated by the factory, and the specifier is mapped to its
/// shim via esbuild's `--alias`. Externals the wrapper never imports simply
/// never resolve their alias, so their shims are not bundled and don't need
/// to be passed to the factory.
fn prepare_iife_factory_entry(input: &Path, externals: &[String]) -> Result<IifeFactoryEntry> {
    let dir = input
        .parent()
        .context("wrapper IIFE input has no parent directory")?;
    let stem = input
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .context("wrapper IIFE input has no file stem")?;
    let input_name = input
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .context("wrapper IIFE input has no file name")?;

    let mut generated = Vec::new();
    let mut aliases = Vec::new();
    let mut store_require = String::new();
    let mut store_populate = String::new();
    if !externals.is_empty() {
        let store_name = format!(".{stem}-iife-store.cjs");
        let store_path = dir.join(&store_name);
        std::fs::write(&store_path, "module.exports = { externals: {} };\n")?;
        generated.push(store_path);
        store_require = format!("var store = require(\"./{store_name}\");\n");
        store_populate = "store.externals = externals || {};\n  ".to_string();

        for (index, spec) in externals.iter().enumerate() {
            let shim_name = format!(".{stem}-iife-shim-{index}.cjs");
            let shim_path = dir.join(&shim_name);
            let message = format!(
                "wasm-bodge IIFE: missing external \"{spec}\"; pass it to the factory function"
            );
            let shim = format!(
                "var store = require(\"./{store_name}\");\n\
                 var dep = store.externals[{spec:?}];\n\
                 if (dep == null) {{\n\
                 \x20 throw new Error({message:?});\n\
                 }}\n\
                 module.exports = dep;\n"
            );
            std::fs::write(&shim_path, &shim)?;
            aliases.push((
                spec.clone(),
                shim_path
                    .canonicalize()
                    .with_context(|| format!("Failed to canonicalize {}", shim_path.display()))?,
            ));
            generated.push(shim_path);
        }
    }

    let entry_name = format!(".{stem}-iife-entry.cjs");
    let entry_path = dir.join(&entry_name);
    let entry = format!(
        "{store_require}\
         module.exports = function createWrapper(externals) {{\n\
         \x20 {store_populate}\
         return require(\"./{input_name}\");\n\
         }};\n"
    );
    std::fs::write(&entry_path, entry)?;
    generated.push(entry_path.clone());

    Ok(IifeFactoryEntry {
        entry: entry_path,
        aliases,
        generated,
    })
}

fn rewrite_virtual_imports(
    output: &Path,
    bindings_specifier: &str,
    slim_bindings_specifier: &str,
) -> Result<()> {
    let mut content = std::fs::read_to_string(output)
        .with_context(|| format!("Failed to read wrapper output {}", output.display()))?;

    // Replace the longer `/slim` specifier first so the base specifier cannot
    // partially rewrite it.
    content = replace_module_specifier(&content, SLIM_BINDINGS_SPECIFIER, slim_bindings_specifier);
    content = replace_module_specifier(&content, BINDINGS_SPECIFIER, bindings_specifier);

    std::fs::write(output, content)
        .with_context(|| format!("Failed to write wrapper output {}", output.display()))?;
    Ok(())
}

fn replace_module_specifier(content: &str, from: &str, to: &str) -> String {
    content
        .replace(&format!("\"{from}\""), &format!("\"{to}\""))
        .replace(&format!("'{from}'"), &format!("'{to}'"))
}

fn emit_declarations(
    package_dir: &Path,
    out_dir: &Path,
    entry: &Path,
    slim_entry: &Path,
    raw_slim_types_rel: &Path,
    user_tsconfig: Option<&Path>,
) -> Result<()> {
    println!("  Emitting wrapper type declarations...");

    let tsc = find_tsc(package_dir)?;
    let types_tmp = package_dir.join("wrapper/.types");
    let _ = std::fs::remove_dir_all(&types_tmp);
    std::fs::create_dir_all(&types_tmp)?;

    let mut entries = vec![entry.to_path_buf()];
    if slim_entry != entry {
        entries.push(slim_entry.to_path_buf());
    }
    let root_dir = common_parent(&entries).context("Failed to find wrapper source root")?;

    let raw_slim_types = out_dir.join(raw_slim_types_rel);
    let config_path = out_dir.join("wrapper/.tsconfig.json");
    let compiler_options = declaration_compiler_options(
        &tsc,
        package_dir,
        user_tsconfig,
        config_path.parent().unwrap_or(out_dir),
        &types_tmp,
        &root_dir,
        &raw_slim_types,
    )?;
    let files: Vec<_> = entries.iter().map(|path| path_to_slash(path)).collect();
    let tsconfig = json!({
        "compilerOptions": compiler_options,
        "files": files
    });
    std::fs::write(&config_path, serde_json::to_string_pretty(&tsconfig)?)?;

    let output = Command::new(&tsc)
        .args(["--project", &config_path.to_string_lossy()])
        .current_dir(package_dir)
        .output()
        .context("Failed to run tsc for wrapper declarations")?;

    if !output.status.success() {
        anyhow::bail!(
            "tsc failed while emitting wrapper declarations:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Preserve declarations for any local modules the public declarations
    // reference, then copy the configured entry declarations to stable names.
    copy_dir_recursive(&types_tmp, &out_dir.join("wrapper"))?;

    copy_declaration(
        &types_tmp,
        &root_dir,
        entry,
        &out_dir.join(targets::paths::wrapper_types()),
    )?;
    copy_declaration(
        &types_tmp,
        &root_dir,
        slim_entry,
        &out_dir.join(targets::paths::wrapper_slim_types()),
    )?;

    let _ = std::fs::remove_dir_all(&types_tmp);
    let _ = std::fs::remove_file(&config_path);
    Ok(())
}

fn declaration_compiler_options(
    tsc: &str,
    package_dir: &Path,
    user_tsconfig: Option<&Path>,
    generated_config_dir: &Path,
    declaration_dir: &Path,
    root_dir: &Path,
    raw_bindings_types: &Path,
) -> Result<Value> {
    let mut options = if let Some(user_tsconfig) = user_tsconfig {
        let output = Command::new(tsc)
            .args([
                "--showConfig",
                "--project",
                &user_tsconfig.to_string_lossy(),
                "--ignoreDeprecations",
                "6.0",
            ])
            .current_dir(package_dir)
            .output()
            .with_context(|| format!("Failed to inspect tsconfig {}", user_tsconfig.display()))?;

        if !output.status.success() {
            anyhow::bail!(
                "tsc --showConfig failed for {}:\nstdout: {}\nstderr: {}",
                user_tsconfig.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let shown_config: Value = serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "Failed to parse tsc --showConfig for {}",
                user_tsconfig.display()
            )
        })?;
        shown_config
            .get("compilerOptions")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    } else {
        serde_json::Map::from_iter([
            ("module".to_string(), json!("ESNext")),
            ("moduleResolution".to_string(), json!("Bundler")),
            ("target".to_string(), json!("ES2022")),
            ("strict".to_string(), json!(true)),
            ("skipLibCheck".to_string(), json!(true)),
            ("esModuleInterop".to_string(), json!(true)),
            ("allowSyntheticDefaultImports".to_string(), json!(true)),
        ])
    };

    let tsconfig_dir = user_tsconfig
        .and_then(Path::parent)
        .unwrap_or(package_dir)
        .to_path_buf();
    let base_url_abs = options
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(|base_url| {
            let base_url = PathBuf::from(base_url);
            if base_url.is_absolute() {
                base_url
            } else {
                tsconfig_dir.join(base_url)
            }
        })
        .unwrap_or(tsconfig_dir);

    // TypeScript 6 deprecates baseUrl, and CI may treat that deprecation as a
    // hard error. Rewrite path mappings so the generated declaration tsconfig
    // can omit baseUrl entirely while preserving the user's aliases. Also drop
    // ignoreDeprecations because values accepted by a newer TypeScript may be
    // rejected by an older TypeScript used locally.
    options.remove("baseUrl");
    options.remove("ignoreDeprecations");

    let existing_paths = match options.remove("paths") {
        Some(Value::Object(paths)) => paths,
        Some(_) => {
            anyhow::bail!("compilerOptions.paths in tsconfig must be an object for wrapper mode")
        }
        None => serde_json::Map::new(),
    };
    let mut paths =
        rewrite_paths_for_generated_config(existing_paths, &base_url_abs, generated_config_dir)?;

    let raw_bindings_types_rel = relative_path_string(generated_config_dir, raw_bindings_types)?;
    let raw_bindings_types_rel = ensure_relative_path_mapping(raw_bindings_types_rel);
    paths.insert(
        BINDINGS_SPECIFIER.to_string(),
        json!([raw_bindings_types_rel.clone()]),
    );
    paths.insert(
        SLIM_BINDINGS_SPECIFIER.to_string(),
        json!([raw_bindings_types_rel]),
    );
    options.insert("paths".to_string(), Value::Object(paths));

    // Raw wasm-bindgen declarations are an input dependency, not wrapper
    // source. Do not let their runtime-library requirements (for example
    // Symbol.dispose with --weak-refs) block declaration emission when the
    // user's tsconfig targets an older standard library.
    options.insert("skipLibCheck".to_string(), json!(true));
    options.insert("declaration".to_string(), json!(true));
    options.insert("emitDeclarationOnly".to_string(), json!(true));
    options.insert("noEmit".to_string(), json!(false));
    options.insert(
        "declarationDir".to_string(),
        json!(path_to_slash(declaration_dir)),
    );
    options.insert("rootDir".to_string(), json!(path_to_slash(root_dir)));

    Ok(Value::Object(options))
}

fn rewrite_paths_for_generated_config(
    paths: serde_json::Map<String, Value>,
    original_base_url: &Path,
    generated_config_dir: &Path,
) -> Result<serde_json::Map<String, Value>> {
    let mut rewritten = serde_json::Map::new();

    for (alias, targets) in paths {
        let Value::Array(targets) = targets else {
            anyhow::bail!("compilerOptions.paths.{alias} must be an array");
        };

        let mut rewritten_targets = Vec::with_capacity(targets.len());
        for target in targets {
            let Some(target) = target.as_str() else {
                anyhow::bail!("compilerOptions.paths.{alias} entries must be strings");
            };
            let target_path = PathBuf::from(target);
            let absolute_target = if target_path.is_absolute() {
                target_path
            } else {
                original_base_url.join(target_path)
            };
            let relative_target = relative_path_string(generated_config_dir, &absolute_target)?;
            rewritten_targets.push(Value::String(ensure_relative_path_mapping(relative_target)));
        }

        rewritten.insert(alias, Value::Array(rewritten_targets));
    }

    Ok(rewritten)
}

fn ensure_relative_path_mapping(path: String) -> String {
    if path.starts_with("./") || path.starts_with("../") || path.starts_with('/') {
        path
    } else {
        format!("./{path}")
    }
}

fn copy_declaration(types_tmp: &Path, root_dir: &Path, entry: &Path, dest: &Path) -> Result<()> {
    let rel = entry
        .strip_prefix(root_dir)
        .with_context(|| format!("{} is not under {}", entry.display(), root_dir.display()))?;
    let mut declaration = types_tmp.join(rel);
    declaration.set_extension("d.ts");

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&declaration, dest).with_context(|| {
        format!(
            "Failed to copy wrapper declaration {} to {}",
            declaration.display(),
            dest.display()
        )
    })?;
    Ok(())
}

fn write_dev_helper(package_dir: &Path, out_dir: &Path, raw_slim_types_rel: &Path) -> Result<()> {
    let helper_dir = package_dir.join(".wasm-bodge");
    std::fs::create_dir_all(&helper_dir)?;

    let raw_slim_types = out_dir.join(raw_slim_types_rel);
    if raw_slim_types.exists() {
        std::fs::copy(&raw_slim_types, helper_dir.join("bindings.d.ts"))?;
        std::fs::copy(&raw_slim_types, helper_dir.join("bindings-slim.d.ts"))?;
    }

    let tsconfig = json!({
        "compilerOptions": {
            "paths": {
                BINDINGS_SPECIFIER: ["./bindings.d.ts"],
                SLIM_BINDINGS_SPECIFIER: ["./bindings-slim.d.ts"]
            }
        }
    });
    std::fs::write(
        helper_dir.join("tsconfig.json"),
        serde_json::to_string_pretty(&tsconfig)?,
    )?;

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }

    Ok(())
}

fn common_parent(paths: &[PathBuf]) -> Option<PathBuf> {
    let first_parent = paths.first()?.parent()?.to_path_buf();
    let mut components: Vec<_> = first_parent.components().collect();

    for path in &paths[1..] {
        let parent_components: Vec<_> = path.parent()?.components().collect();
        let common_len = components
            .iter()
            .zip(parent_components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        components.truncate(common_len);
    }

    let mut result = PathBuf::new();
    for component in components {
        result.push(component.as_os_str());
    }
    Some(result)
}

fn relative_path_string(from_dir: &Path, to: &Path) -> Result<String> {
    let rel = pathdiff::diff_paths(to, from_dir)
        .with_context(|| format!("Failed to compute path to {}", to.display()))?;
    Ok(path_to_slash(&rel))
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn find_esbuild() -> Result<String> {
    let candidates = [
        "esbuild",
        "./node_modules/.bin/esbuild",
        "../node_modules/.bin/esbuild",
    ];

    for candidate in candidates {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Ok(candidate.to_string());
        }
    }

    anyhow::bail!("esbuild not found. Wrapper mode requires esbuild in PATH or node_modules/.bin")
}

fn find_tsc(package_dir: &Path) -> Result<String> {
    let candidates = [
        "tsc".to_string(),
        package_dir
            .join("node_modules/.bin/tsc")
            .display()
            .to_string(),
        package_dir
            .join("../node_modules/.bin/tsc")
            .display()
            .to_string(),
    ];

    for candidate in candidates {
        if Command::new(&candidate)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "tsc not found. Wrapper mode requires TypeScript (install `typescript` or make `tsc` available in PATH)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_package_json(test_name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wasm-bodge-wrapper-config-test-{test_name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("package.json");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn externals_rejects_wildcards() {
        let path = write_package_json(
            "rejects-wildcards",
            r#"{
              "wasm-bodge": {
                "wrapper": {
                  "entry": "./src/index.ts",
                  "externals": ["@automerge/automerge/*"]
                }
              }
            }"#,
        );
        let err = read_config(&path).unwrap_err();
        assert!(
            err.to_string().contains("does not support wildcards"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn externals_accepts_exact_specifiers() {
        let path = write_package_json(
            "accepts-exact",
            r#"{
              "wasm-bodge": {
                "wrapper": {
                  "entry": "./src/index.ts",
                  "externals": ["@automerge/automerge", "@automerge/automerge/next"]
                }
              }
            }"#,
        );
        let config = read_config(&path).unwrap().unwrap();
        assert_eq!(
            config.externals,
            vec![
                "@automerge/automerge".to_string(),
                "@automerge/automerge/next".to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn externals_defaults_to_empty() {
        let path = write_package_json(
            "defaults-empty",
            r#"{
              "wasm-bodge": {
                "wrapper": { "entry": "./src/index.ts" }
              }
            }"#,
        );
        let config = read_config(&path).unwrap().unwrap();
        assert!(config.externals.is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
