//! Declarative definitions of all build targets and their relationships.
//!
//! This module is the single source of truth for understanding how wasm-bodge
//! builds packages. Each `Target` defines:
//! - Which wasm-bindgen target it uses
//! - How to generate its ESM and CJS entrypoints
//! - Which package.json export conditions map to it
//!
//! To understand how a specific export is built, find the relevant `Target`
//! definition below.

use std::fmt;

/// The wasm-bindgen CLI targets we use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmBindgenTarget {
    /// `--target nodejs` - CommonJS output with fs-based wasm loading
    Nodejs,
    /// `--target web` - ESM output with manual initialization
    Web,
    /// `--target bundler` - ESM output expecting bundler to handle wasm
    Bundler,
}

impl WasmBindgenTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Nodejs => "nodejs",
            Self::Web => "web",
            Self::Bundler => "bundler",
        }
    }

    /// All targets that need to be built
    pub fn all() -> &'static [WasmBindgenTarget] {
        &[Self::Nodejs, Self::Web, Self::Bundler]
    }

    /// Directory name under wasm_bindgen/
    pub fn dir_name(&self) -> &'static str {
        self.as_str()
    }
}

impl fmt::Display for WasmBindgenTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Which wasm binary variant an entrypoint uses.
///
/// The Optimized variant is produced by `wasm-opt -O4 --all-features`, which
/// strips debug symbols. The Debug variant is produced by
/// `wasm-opt -O4 --all-features -g`, which preserves DWARF debug info and the
/// name section so the wasm can be debugged in browser devtools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmVariant {
    /// Optimized wasm (post-wasm-opt, debug symbols stripped)
    Optimized,
    /// Debug wasm (post-wasm-opt -g, debug symbols preserved)
    Debug,
}

impl std::fmt::Display for WasmVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WasmVariant::Optimized => write!(f, "Optimized"),
            WasmVariant::Debug => write!(f, "Debug"),
        }
    }
}

impl WasmVariant {
    /// All variants we generate
    pub fn all() -> &'static [WasmVariant] {
        &[Self::Optimized, Self::Debug]
    }

    /// Prefix for entrypoint file stems: "" for optimized, "debug-" for debug.
    pub fn file_prefix(&self) -> &'static str {
        match self {
            Self::Optimized => "",
            Self::Debug => "debug-",
        }
    }

    /// Suffix for wasm_bindgen/ output directories: "" for optimized, "-debug" for debug.
    pub fn dir_suffix(&self) -> &'static str {
        match self {
            Self::Optimized => "",
            Self::Debug => "-debug",
        }
    }

    /// Whether this is the debug variant.
    pub fn is_debug(&self) -> bool {
        matches!(self, Self::Debug)
    }
}

/// How an entrypoint initializes the wasm module.
#[derive(Debug, Clone, Copy)]
pub enum InitStrategy {
    /// Auto-initializes by reading wasm from disk via node:fs and calling initSync
    NodeFsSync,
    /// Auto-initializes by embedding wasm as base64
    Base64Embedded,
    /// Auto-initializes via synchronous wasm import (workerd)
    SyncWasmImport,
    /// Imports wasm via bundler target, injects into web target bindings
    BundlerShim,
    /// No initialization - user must call initSync manually
    Manual,
}

/// A runtime environment we support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    /// Node.js (both ESM and CJS)
    Node,
    /// Browsers without a bundler (uses base64 embedded wasm)
    Web,
    /// Bundlers like Webpack, Vite, Rollup
    Bundler,
    /// Cloudflare Workers (workerd runtime)
    Workerd,
    /// Script tag usage (IIFE)
    #[expect(dead_code)]
    Iife,
    /// Manual initialization (escape hatch)
    Slim,
}

impl Environment {
    /// All environments we generate entrypoints for
    pub fn all() -> &'static [Environment] {
        &[
            Self::Node,
            Self::Web,
            Self::Bundler,
            Self::Workerd,
            Self::Slim,
            // Note: IIFE is handled specially (bundled from Web)
        ]
    }

    /// The base filename for this environment's entrypoints (without extension)
    pub fn file_stem(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Web => "web",
            Self::Bundler => "bundler",
            Self::Workerd => "workerd",
            Self::Iife => "index", // in iife/ subdir
            Self::Slim => "slim",
        }
    }

    /// Which wasm-bindgen target this environment's entrypoint uses
    #[cfg(test)]
    pub fn wasm_bindgen_target(&self) -> WasmBindgenTarget {
        match self {
            Self::Node => WasmBindgenTarget::Web,
            Self::Web => WasmBindgenTarget::Web,
            Self::Bundler => WasmBindgenTarget::Web,
            Self::Workerd => WasmBindgenTarget::Web,
            Self::Iife => WasmBindgenTarget::Web, // bundled from web.js
            Self::Slim => WasmBindgenTarget::Web,
        }
    }

    /// How this environment initializes the wasm module
    pub fn init_strategy(&self) -> InitStrategy {
        match self {
            Self::Node => InitStrategy::NodeFsSync,
            Self::Web => InitStrategy::Base64Embedded,
            Self::Bundler => InitStrategy::BundlerShim,
            Self::Workerd => InitStrategy::SyncWasmImport,
            Self::Iife => InitStrategy::Base64Embedded,
            Self::Slim => InitStrategy::Manual,
        }
    }

    /// Whether this environment needs esbuild bundling for CJS.
    ///
    /// Some environments (like Workerd) use a different environment for CJS
    /// (specified in ROOT_EXPORT_MAPPING), so they don't need their own bundle.
    pub fn needs_cjs_bundle(&self) -> bool {
        match self {
            // Node and Slim generate CJS directly (not bundled)
            Self::Node | Self::Slim => false,
            // Web uses web target (ESM) so needs bundling for CJS
            Self::Web => true,
            // Bundler CJS falls back to web.cjs (doesn't need its own bundle)
            Self::Bundler => false,
            // Workerd CJS falls back to web.cjs (specified in ROOT_EXPORT_MAPPING)
            Self::Workerd => false,
            // IIFE doesn't have a CJS variant
            Self::Iife => false,
        }
    }
}

/// An export condition in package.json (e.g., "node", "browser", "import")
#[derive(Debug, Clone, Copy)]
pub enum ExportCondition {
    /// "node" - Node.js runtime
    Node,
    /// "browser" - Browser environment (typically via bundler)
    Browser,
    /// "workerd" - Cloudflare Workers runtime
    Workerd,
    /// "import" - ES Module import (fallback)
    Import,
    /// "require" - CommonJS require (fallback)
    Require,
}

impl ExportCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Browser => "browser",
            Self::Workerd => "workerd",
            Self::Import => "import",
            Self::Require => "require",
        }
    }
}

/// Defines how a package.json export condition maps to environments.
#[derive(Debug, Clone, Copy)]
pub struct ExportMapping {
    /// The condition name in package.json (e.g., "node", "browser")
    pub condition: ExportCondition,
    /// Environment used for ESM imports
    pub esm: Environment,
    /// Environment used for CJS requires
    pub cjs: Environment,
}

/// Mapping from export conditions to environments for the root "." export.
///
/// This defines which environment handles each condition. The order matters
/// for how package.json exports are structured (more specific conditions first).
pub const ROOT_EXPORT_MAPPING: &[ExportMapping] = &[
    // More specific conditions first
    ExportMapping {
        condition: ExportCondition::Workerd,
        esm: Environment::Workerd,
        cjs: Environment::Web,
    },
    ExportMapping {
        condition: ExportCondition::Node,
        esm: Environment::Node,
        cjs: Environment::Node,
    },
    ExportMapping {
        condition: ExportCondition::Browser,
        esm: Environment::Bundler,
        cjs: Environment::Web,
    },
    // Fallbacks (no specific condition, just import/require)
    ExportMapping {
        condition: ExportCondition::Import,
        esm: Environment::Web,
        cjs: Environment::Web,
    },
    ExportMapping {
        condition: ExportCondition::Require,
        esm: Environment::Web,
        cjs: Environment::Web,
    },
];

// ============================================================================
// Path helpers - centralized path construction using PathBuf
// ============================================================================

/// Paths relative to the output directory (dist/)
pub mod paths {
    use super::{Environment, WasmBindgenTarget, WasmVariant};
    use std::path::PathBuf;

    /// Path to wasm-bindgen output directory: wasm_bindgen/{target}/
    pub fn wasm_bindgen_dir(target: WasmBindgenTarget) -> PathBuf {
        PathBuf::from("wasm_bindgen").join(target.dir_name())
    }

    /// Path to ESM entrypoint: esm/{prefix}{env}.js
    pub fn esm_entrypoint(env: Environment, variant: WasmVariant) -> PathBuf {
        PathBuf::from("esm").join(format!("{}{}.js", variant.file_prefix(), env.file_stem()))
    }

    /// Path to CJS entrypoint: cjs/{prefix}{env}.cjs
    pub fn cjs_entrypoint(env: Environment, variant: WasmVariant) -> PathBuf {
        PathBuf::from("cjs").join(format!("{}{}.cjs", variant.file_prefix(), env.file_stem()))
    }

    /// Path to IIFE bundle: iife/index.js or iife/debug.js
    pub fn iife_bundle(variant: WasmVariant) -> PathBuf {
        match variant {
            WasmVariant::Optimized => PathBuf::from("iife/index.js"),
            WasmVariant::Debug => PathBuf::from("iife/debug.js"),
        }
    }

    /// Path to base64 wasm module (ESM): esm/{prefix}wasm-base64.js
    pub fn wasm_base64_esm(variant: WasmVariant) -> PathBuf {
        PathBuf::from(format!("esm/{}wasm-base64.js", variant.file_prefix()))
    }

    /// Path to base64 wasm module (CJS): cjs/{prefix}wasm-base64.cjs
    pub fn wasm_base64_cjs(variant: WasmVariant) -> PathBuf {
        PathBuf::from(format!("cjs/{}wasm-base64.cjs", variant.file_prefix()))
    }

    /// Path to CJS web bindings bundle: cjs/{prefix}web-bindings.cjs
    ///
    /// Bundled per variant -- wasm-opt renames wasm exports in the optimized
    /// variant (e.g. `__wbindgen_malloc` -> `__wbindgen_export`), so the JS
    /// bindings emitted by wasm-bindgen differ between variants and cannot be
    /// shared.
    pub fn cjs_web_bindings(variant: WasmVariant) -> PathBuf {
        PathBuf::from(format!("cjs/{}web-bindings.cjs", variant.file_prefix()))
    }

    /// Path to TypeScript declarations: index.d.ts
    pub fn types() -> PathBuf {
        PathBuf::from("index.d.ts")
    }

    /// Path to standalone wasm file: {package_name}.wasm or {package_name}-debug.wasm
    pub fn standalone_wasm(package_name: &str, variant: WasmVariant) -> PathBuf {
        match variant {
            WasmVariant::Optimized => PathBuf::from(format!("{}.wasm", package_name)),
            WasmVariant::Debug => PathBuf::from(format!("{}-debug.wasm", package_name)),
        }
    }
}

// ============================================================================
// Entrypoint content generation
// ============================================================================

/// Generates the JavaScript content for an ESM entrypoint.
///
/// Each variant references its own wasm-bindgen JS output (wasm_bindgen/web[-debug]/)
/// because wasm-opt renames wasm exports in the optimized variant so the JS
/// bindings diverge between variants.
pub fn generate_esm_entrypoint(env: Environment, wasm_name: &str, variant: WasmVariant) -> String {
    let web_dir = format!("wasm_bindgen/web{}", variant.dir_suffix());
    let bundler_wasm_dir = format!("wasm_bindgen/bundler{}", variant.dir_suffix());
    let base64_import = format!("./{}wasm-base64.js", variant.file_prefix());

    match env.init_strategy() {
        InitStrategy::NodeFsSync => {
            // Read wasm from disk and initialize synchronously
            format!(
                r#"import {{ initSync }} from '../{web_dir}/{name}.js';
import {{ readFileSync }} from 'node:fs';
import {{ fileURLToPath }} from 'node:url';
import {{ dirname, join }} from 'node:path';
const __dirname = dirname(fileURLToPath(import.meta.url));
initSync({{ module: readFileSync(join(__dirname, '../{web_dir}/{name}_bg.wasm')) }});
export * from '../{web_dir}/{name}.js';
"#,
                name = wasm_name,
                web_dir = web_dir,
            )
        }
        InitStrategy::Base64Embedded => {
            // Import base64, decode, init, then re-export
            format!(
                r#"import {{ initSync }} from '../{web_dir}/{name}.js';
import {{ wasmBase64 }} from '{base64_import}';
const bytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0));
initSync({{ module: bytes }});
export * from '../{web_dir}/{name}.js';
"#,
                name = wasm_name,
                web_dir = web_dir,
                base64_import = base64_import,
            )
        }
        InitStrategy::SyncWasmImport => {
            // Synchronously import wasm module (workerd)
            format!(
                r#"import * as exports from '../{web_dir}/{name}.js';
import {{ initSync }} from '../{web_dir}/{name}.js';
import wasmModule from '../{web_dir}/{name}_bg.wasm';
initSync({{ module: wasmModule }});
export * from '../{web_dir}/{name}.js';
"#,
                name = wasm_name,
                web_dir = web_dir,
            )
        }
        InitStrategy::BundlerShim => {
            // Import wasm via bundler target (bundler handles loading), inject
            // into web target bindings so bundler and slim share wasm state
            // within this variant. The bundler resolves wasm imports relative
            // to the _bg.js, so both _bg.js and _bg.wasm come from the same
            // directory (bundler or bundler-debug).
            format!(
                r#"import {{ __wbg_set_wasm as __bundler_set_wasm }} from '../{bundler_dir}/{name}_bg.js';
import * as wasmExports from '../{bundler_dir}/{name}_bg.wasm';
import {{ __wbg_set_wasm }} from '../{web_dir}/{name}.js';
__bundler_set_wasm(wasmExports);
wasmExports.__wbindgen_start();
__wbg_set_wasm(wasmExports);
export * from '../{web_dir}/{name}.js';
"#,
                name = wasm_name,
                web_dir = web_dir,
                bundler_dir = bundler_wasm_dir,
            )
        }
        InitStrategy::Manual => {
            // Re-export without initialization (user calls initSync).
            format!(
                "export * from '../{web_dir}/{name}.js';\nexport {{ default }} from '../{web_dir}/{name}.js';\n",
                name = wasm_name,
                web_dir = web_dir,
            )
        }
    }
}

/// Generates the JavaScript content for a CJS entrypoint (if not bundled).
///
/// Each variant has its own bundled `web-bindings.cjs` (or
/// `debug-web-bindings.cjs`) because the JS bindings differ between variants.
pub fn generate_cjs_entrypoint(
    env: Environment,
    wasm_name: &str,
    variant: WasmVariant,
) -> Option<String> {
    let wasm_dir = format!("wasm_bindgen/web{}", variant.dir_suffix());
    let bindings_require = format!("./{}web-bindings.cjs", variant.file_prefix());

    match env {
        Environment::Node => {
            // Load variant's web bindings, read wasm from disk, initialize
            Some(format!(
                r#"const bindings = require('{bindings_require}');
const fs = require('fs');
const path = require('path');
bindings.initSync({{ module: fs.readFileSync(path.join(__dirname, '../{wasm_dir}/{name}_bg.wasm')) }});
module.exports = bindings;
"#,
                name = wasm_name,
                wasm_dir = wasm_dir,
                bindings_require = bindings_require,
            ))
        }
        Environment::Slim => {
            // Just re-export the variant's web bindings (no initialization).
            Some(format!(
                "module.exports = require('{bindings_require}');\n",
                bindings_require = bindings_require,
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_browser_export_mapping() {
        // "How is the browser export built?"
        let mapping = ROOT_EXPORT_MAPPING
            .iter()
            .find(|m| matches!(m.condition, ExportCondition::Browser))
            .unwrap();

        // Browser uses Bundler environment for ESM
        assert_eq!(mapping.esm, Environment::Bundler);
        // Browser uses Web environment for CJS (fallback)
        assert_eq!(mapping.cjs, Environment::Web);

        // Bundler environment uses web target (shares wasm state with slim)
        assert_eq!(mapping.esm.wasm_bindgen_target(), WasmBindgenTarget::Web);

        // The ESM entrypoint path
        assert_eq!(
            paths::esm_entrypoint(mapping.esm, WasmVariant::Optimized),
            PathBuf::from("esm/bundler.js")
        );
        assert_eq!(
            paths::esm_entrypoint(mapping.esm, WasmVariant::Debug),
            PathBuf::from("esm/debug-bundler.js")
        );
    }

    #[test]
    fn test_workerd_export_mapping() {
        let mapping = ROOT_EXPORT_MAPPING
            .iter()
            .find(|m| matches!(m.condition, ExportCondition::Workerd))
            .unwrap();

        assert_eq!(mapping.esm, Environment::Workerd);
        assert_eq!(mapping.cjs, Environment::Web); // falls back to web for CJS

        // Workerd uses web target with sync wasm import
        assert_eq!(mapping.esm.wasm_bindgen_target(), WasmBindgenTarget::Web);
        assert!(matches!(
            mapping.esm.init_strategy(),
            InitStrategy::SyncWasmImport
        ));
    }

    #[test]
    fn test_variant_paths() {
        assert_eq!(WasmVariant::Optimized.file_prefix(), "");
        assert_eq!(WasmVariant::Debug.file_prefix(), "debug-");
        assert_eq!(WasmVariant::Optimized.dir_suffix(), "");
        assert_eq!(WasmVariant::Debug.dir_suffix(), "-debug");

        assert_eq!(
            paths::standalone_wasm("my-pkg", WasmVariant::Optimized),
            PathBuf::from("my-pkg.wasm")
        );
        assert_eq!(
            paths::standalone_wasm("my-pkg", WasmVariant::Debug),
            PathBuf::from("my-pkg-debug.wasm")
        );

        assert_eq!(
            paths::wasm_base64_esm(WasmVariant::Debug),
            PathBuf::from("esm/debug-wasm-base64.js")
        );
        assert_eq!(
            paths::iife_bundle(WasmVariant::Debug),
            PathBuf::from("iife/debug.js")
        );
    }

    #[test]
    fn test_debug_entrypoint_uses_web_debug_js() {
        // Each variant references its own wasm-bindgen JS output because
        // wasm-opt rewrites wasm export names in the optimized variant and the
        // JS bindings diverge as a result.
        let node_debug = generate_esm_entrypoint(Environment::Node, "my_crate", WasmVariant::Debug);
        assert!(node_debug.contains("from '../wasm_bindgen/web-debug/my_crate.js'"));
        assert!(node_debug.contains("../wasm_bindgen/web-debug/my_crate_bg.wasm"));
        assert!(!node_debug.contains("wasm_bindgen/web/my_crate.js"));

        let bundler_debug =
            generate_esm_entrypoint(Environment::Bundler, "my_crate", WasmVariant::Debug);
        assert!(bundler_debug.contains("'../wasm_bindgen/bundler-debug/my_crate_bg.js'"));
        assert!(bundler_debug.contains("'../wasm_bindgen/bundler-debug/my_crate_bg.wasm'"));
        assert!(bundler_debug.contains("'../wasm_bindgen/web-debug/my_crate.js'"));

        // Optimized keeps its old references
        let node_opt =
            generate_esm_entrypoint(Environment::Node, "my_crate", WasmVariant::Optimized);
        assert!(node_opt.contains("from '../wasm_bindgen/web/my_crate.js'"));
    }

    /// Fail if any generated entrypoint uses the deprecated
    /// positional-bytes form of `initSync`
    /// (wasm-bindgen deprecated it in 0.2.87 in favor of `initSync({ module: ... })`).
    #[test]
    fn test_no_deprecated_init_sync_form() {
        let re = regex::Regex::new(r"initSync\(\s*([^{\s])").unwrap();

        let mut generated = Vec::new();
        for env in Environment::all() {
            for variant in WasmVariant::all() {
                generated.push((
                    format!("esm[{:?}, {:?}]", env, variant),
                    generate_esm_entrypoint(*env, "my_crate", *variant),
                ));
                if let Some(cjs) = generate_cjs_entrypoint(*env, "my_crate", *variant) {
                    generated.push((format!("cjs[{:?}, {:?}]", env, variant), cjs));
                }
            }
        }

        for (label, src) in &generated {
            if let Some(cap) = re.captures(src) {
                panic!(
                    "{} uses the deprecated positional-bytes form of initSync; \
                     matched `initSync({}...)`. Use `initSync({{ module: ... }})` \
                     instead.\n--- generated source ---\n{}",
                    label, &cap[1], src
                );
            }
        }
    }
}
