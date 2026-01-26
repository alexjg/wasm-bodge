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

/// How an entrypoint initializes the wasm module.
#[derive(Debug, Clone, Copy)]
pub enum InitStrategy {
    /// Auto-initializes via the wasm-bindgen output (nodejs target)
    AutoNodejs,
    /// Auto-initializes by embedding wasm as base64
    Base64Embedded,
    /// Auto-initializes via synchronous wasm import (workerd)
    SyncWasmImport,
    /// Re-exports bundler target (bundler handles wasm loading)
    BundlerPassthrough,
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
    pub fn wasm_bindgen_target(&self) -> WasmBindgenTarget {
        match self {
            Self::Node => WasmBindgenTarget::Nodejs,
            Self::Web => WasmBindgenTarget::Web,
            Self::Bundler => WasmBindgenTarget::Bundler,
            Self::Workerd => WasmBindgenTarget::Web,
            Self::Iife => WasmBindgenTarget::Web, // bundled from web.js
            Self::Slim => WasmBindgenTarget::Web,
        }
    }

    /// How this environment initializes the wasm module
    pub fn init_strategy(&self) -> InitStrategy {
        match self {
            Self::Node => InitStrategy::AutoNodejs,
            Self::Web => InitStrategy::Base64Embedded,
            Self::Bundler => InitStrategy::BundlerPassthrough,
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
            // Node can directly require the .cjs file
            Self::Node => false,
            // Web and Slim use web target (ESM) so need bundling for CJS
            Self::Web | Self::Slim => true,
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
    use super::{Environment, WasmBindgenTarget};
    use std::path::PathBuf;

    /// Path to wasm-bindgen output directory: wasm_bindgen/{target}/
    pub fn wasm_bindgen_dir(target: WasmBindgenTarget) -> PathBuf {
        PathBuf::from("wasm_bindgen").join(target.dir_name())
    }

    /// Path to wasm-bindgen JS file: wasm_bindgen/{target}/{name}.js (or .cjs for nodejs)
    pub fn wasm_bindgen_js(target: WasmBindgenTarget, wasm_name: &str) -> PathBuf {
        let ext = if target == WasmBindgenTarget::Nodejs {
            "cjs"
        } else {
            "js"
        };
        wasm_bindgen_dir(target).join(format!("{}.{}", wasm_name, ext))
    }

    /// Path to ESM entrypoint: esm/{env}.js
    pub fn esm_entrypoint(env: Environment) -> PathBuf {
        PathBuf::from("esm").join(format!("{}.js", env.file_stem()))
    }

    /// Path to CJS entrypoint: cjs/{env}.cjs
    pub fn cjs_entrypoint(env: Environment) -> PathBuf {
        PathBuf::from("cjs").join(format!("{}.cjs", env.file_stem()))
    }

    /// Path to IIFE bundle: iife/index.js
    pub fn iife_bundle() -> PathBuf {
        PathBuf::from("iife/index.js")
    }

    /// Path to base64 wasm module (ESM): esm/wasm-base64.js
    pub fn wasm_base64_esm() -> PathBuf {
        PathBuf::from("esm/wasm-base64.js")
    }

    /// Path to base64 wasm module (CJS): cjs/wasm-base64.cjs
    pub fn wasm_base64_cjs() -> PathBuf {
        PathBuf::from("cjs/wasm-base64.cjs")
    }

    /// Path to TypeScript declarations: index.d.ts
    pub fn types() -> PathBuf {
        PathBuf::from("index.d.ts")
    }

    /// Path to standalone wasm file: {package_name}.wasm
    pub fn standalone_wasm(package_name: &str) -> PathBuf {
        PathBuf::from(format!("{}.wasm", package_name))
    }
}

// ============================================================================
// Entrypoint content generation
// ============================================================================

/// Generates the JavaScript content for an ESM entrypoint.
pub fn generate_esm_entrypoint(env: Environment, wasm_name: &str) -> String {
    let target = env.wasm_bindgen_target();

    match env.init_strategy() {
        InitStrategy::AutoNodejs => {
            // Re-export from nodejs target (which auto-initializes)
            let path = paths::wasm_bindgen_js(target, wasm_name);
            format!("export * from '../{}';\n", path.display())
        }
        InitStrategy::Base64Embedded => {
            // Import base64, decode, init, then re-export
            format!(
                r#"import {{ initSync }} from '../wasm_bindgen/web/{name}.js';
import {{ wasmBase64 }} from './wasm-base64.js';
const bytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0));
initSync(bytes);
export * from '../wasm_bindgen/web/{name}.js';
"#,
                name = wasm_name
            )
        }
        InitStrategy::SyncWasmImport => {
            // Synchronously import wasm module (workerd)
            format!(
                r#"import * as exports from '../wasm_bindgen/web/{name}.js';
import {{ initSync }} from '../wasm_bindgen/web/{name}.js';
import wasmModule from '../wasm_bindgen/web/{name}_bg.wasm';
initSync({{ module: wasmModule }});
export * from '../wasm_bindgen/web/{name}.js';
"#,
                name = wasm_name
            )
        }
        InitStrategy::BundlerPassthrough => {
            // Just re-export from bundler target
            let path = paths::wasm_bindgen_js(target, wasm_name);
            format!("export * from '../{}';\n", path.display())
        }
        InitStrategy::Manual => {
            // Re-export without initialization (user calls initSync)
            format!(
                "export * from '../wasm_bindgen/web/{name}.js';\nexport {{ default }} from '../wasm_bindgen/web/{name}.js';\n",
                name = wasm_name
            )
        }
    }
}

/// Generates the JavaScript content for a CJS entrypoint (if not bundled).
pub fn generate_cjs_entrypoint(env: Environment, wasm_name: &str) -> Option<String> {
    // Only Node has a simple CJS entrypoint; others are bundled from ESM
    if env == Environment::Node {
        let path = paths::wasm_bindgen_js(WasmBindgenTarget::Nodejs, wasm_name);
        Some(format!(
            "module.exports = require('../{}');\n",
            path.display()
        ))
    } else {
        None
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

        // Bundler environment uses the bundler wasm-bindgen target
        assert_eq!(
            mapping.esm.wasm_bindgen_target(),
            WasmBindgenTarget::Bundler
        );

        // The ESM entrypoint path
        assert_eq!(
            paths::esm_entrypoint(mapping.esm),
            PathBuf::from("esm/bundler.js")
        );

        // Which re-exports from
        assert_eq!(
            paths::wasm_bindgen_js(WasmBindgenTarget::Bundler, "my_lib"),
            PathBuf::from("wasm_bindgen/bundler/my_lib.js")
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
}
