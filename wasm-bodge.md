# wasm-bodge

A tool that takes wasm-bindgen output and wraps it with the entrypoint magic
needed to make it work seamlessly across all JavaScript runtimes.

## Table of Contents

1. [Problem and Solution](#1-problem-and-solution)
2. [Quick Start](#2-quick-start)
3. [Consumer Guide](#3-consumer-guide)
4. [Generated Output](#4-generated-output)
5. [Build Pipeline](#5-build-pipeline)
6. [Configuration](#6-configuration)
7. [Testing](#7-testing)

---

## 1. Problem and Solution

### The Problem

wasm-bindgen generates JavaScript bindings for your Rust WebAssembly code, but
the generated package assumes a specific loading strategy. In reality, wasm
loading varies dramatically across JavaScript runtimes:

| Environment | How wasm is loaded |
|-------------|-------------------|
| **Bundlers** (Webpack, Vite) | Bundler handles wasm as an asset |
| **Node.js ESM** | Import wasm as module |
| **Node.js CJS** | Sync load from filesystem |
| **Cloudflare Workers** | Explicit `initSync` with wasm module |
| **Browser (no bundler)** | Fetch and instantiate manually |
| **`<script>` tag** | Needs IIFE bundle |

Publishing raw wasm-bindgen output forces users to configure their environment
specifically for your package.

### The Solution

wasm-bodge takes wasm-bindgen output and generates a complete npm package with
**conditional exports** that automatically select the right loading strategy:

```bash
wasm-bodge build --crate ./my-rust-lib
```

Users simply import your package and it works everywhere:

```javascript
import { myFunction } from "my-wasm-lib"
```

---

## 2. Quick Start

### Prerequisites

- Rust toolchain with `wasm32-unknown-unknown` target
- `wasm-bindgen-cli` installed
- Node.js (for esbuild, used to bundle CJS/IIFE)

### Setup

1. Create a `package.json` template in your project:

```json
{
  "name": "my-wasm-lib",
  "version": "1.0.0",
  "license": "MIT",
  "description": "My wasm library"
}
```

2. Build:

```bash
wasm-bodge build --crate ../rust/my-wasm-lib
```

3. Publish:

```bash
cd dist && npm publish
```

---

## 3. Consumer Guide

This section describes what **consumers** of the generated package experience.

### 3.1 Default Import (Auto-Initialize)

Most users simply import and use:

```javascript
import { myFunction, MyClass } from "my-wasm-lib"

myFunction()  // Wasm already initialized
```

The package.json exports map routes to the correct entrypoint automatically:
- Webpack/Vite/Rollup -> `esm/bundler.js`
- Node.js ESM -> `esm/node.js`
- Node.js CJS -> `cjs/node.cjs`
- Cloudflare Workers -> `esm/workerd.js`
- Unknown/fallback -> `esm/web.js` (embedded base64 wasm)

### 3.2 Manual Initialization (`/slim`)

For libraries or when you need control over initialization timing:

```javascript
import { init, initSync, myFunction } from "my-wasm-lib/slim"

// Option 1: Async initialization
await init()
myFunction()

// Option 2: Async with custom source
await init(fetch("/custom/path/to.wasm"))

// Option 3: Sync initialization  
initSync(wasmBytes)
myFunction()
```

### 3.3 Raw Wasm Access

```javascript
// Import raw wasm URL (for bundlers)
import wasmUrl from "my-wasm-lib/wasm"

// Import base64-encoded wasm (for restricted environments)
import { wasmBase64 } from "my-wasm-lib/wasm-base64"
```

### 3.4 IIFE Bundle

For `<script>` tag usage:

```html
<script src="https://unpkg.com/my-wasm-lib/iife/index.js"></script>
<script>
  MyWasmLib.myFunction()
</script>
```

### 3.5 Available Exports

| Import Path | Description |
|-------------|-------------|
| `my-wasm-lib` | Full API with auto-initialized wasm |
| `my-wasm-lib/slim` | Full API, wasm not initialized |
| `my-wasm-lib/wasm` | Raw `.wasm` file |
| `my-wasm-lib/wasm-base64` | Base64-encoded wasm as ES module |

---

## 4. Generated Output

### 4.1 Directory Structure

```
dist/
├── package.json              # Template merged with generated exports
├── index.d.ts                # TypeScript declarations (from wasm-bindgen)
├── {name}.wasm               # Raw wasm file
├── esm/
│   ├── bundler.js            # For bundlers (Webpack, Vite, etc.)
│   ├── node.js               # For Node.js ESM
│   ├── workerd.js            # For Cloudflare Workers
│   ├── web.js                # Self-initializing with base64 wasm
│   ├── slim.js               # No auto-initialization
│   └── wasm-base64.js        # Base64-encoded wasm
├── cjs/
│   ├── node.cjs              # For Node.js CJS
│   ├── web.cjs               # Self-initializing with base64 wasm
│   └── slim.cjs              # No auto-initialization
├── iife/
│   └── index.js              # For <script> tags
└── wasm_bindgen/             # Raw wasm-bindgen output (internal)
    ├── bundler/
    ├── nodejs/
    └── web/
```

### 4.2 Generated package.json

The tool merges your template with generated fields:

```json
{
  "name": "my-wasm-lib",
  "version": "1.0.0",
  "license": "MIT",
  "description": "My wasm library",
  
  "type": "module",
  "main": "./cjs/node.cjs",
  "module": "./esm/bundler.js",
  "types": "./index.d.ts",
  "files": [
    "*.wasm",
    "*.d.ts",
    "esm",
    "cjs",
    "iife"
  ],
  "exports": {
    ".": {
      "types": "./index.d.ts",
      "workerd": {
        "import": "./esm/workerd.js",
        "require": "./cjs/web.cjs"
      },
      "node": {
        "import": "./esm/node.js",
        "require": "./cjs/node.cjs"
      },
      "browser": {
        "import": "./esm/bundler.js",
        "require": "./cjs/web.cjs"
      },
      "import": "./esm/web.js",
      "require": "./cjs/web.cjs"
    },
    "./slim": {
      "types": "./index.d.ts",
      "import": "./esm/slim.js",
      "require": "./cjs/slim.cjs"
    },
    "./wasm": "./{name}.wasm",
    "./wasm-base64": {
      "import": "./esm/wasm-base64.js",
      "require": "./cjs/wasm-base64.cjs"
    }
  }
}
```

### 4.3 Entrypoint Contents

**esm/bundler.js** - For bundlers that handle wasm imports:
```javascript
export * from '../wasm_bindgen/bundler/{name}.js'
```

**esm/node.js** - For Node.js ESM:
```javascript
export * from '../wasm_bindgen/nodejs/{name}.js'
```

**esm/workerd.js** - For Cloudflare Workers:
```javascript
import * as exports from '../wasm_bindgen/web/{name}.js'
import { initSync } from '../wasm_bindgen/web/{name}.js'
import wasmModule from '../wasm_bindgen/web/{name}_bg.wasm'
initSync({ module: wasmModule })
export * from '../wasm_bindgen/web/{name}.js'
```

**esm/web.js** - Self-initializing with embedded base64:
```javascript
import { initSync } from '../wasm_bindgen/web/{name}.js'
import { wasmBase64 } from './wasm-base64.js'
const bytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0))
initSync(bytes)
export * from '../wasm_bindgen/web/{name}.js'
```

**esm/slim.js** - No initialization (re-exports wasm-bindgen's init functions):
```javascript
export * from '../wasm_bindgen/web/{name}.js'
```

**esm/wasm-base64.js**:
```javascript
export const wasmBase64 = "AGFzbQEAAAA..."
```

---

## 5. Build Pipeline

### 5.1 Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  Phase 1: Build Wasm                                            │
│  cargo build --target wasm32-unknown-unknown --release          │
│  wasm-bindgen for targets: bundler, nodejs, web                 │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 2: Post-Process                                          │
│  - Copy web/ -> workerd/                                        │
│  - Apply @vite-ignore fix to web target                         │
│  - Generate base64 wasm module                                  │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 3: Generate Entrypoints                                  │
│  - Generate esm/*.js entrypoints                                │
│  - Bundle cjs/*.cjs with esbuild                                │
│  - Bundle iife/index.js with esbuild                            │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│  Phase 4: Finalize Package                                      │
│  - Copy template package.json, merge exports                    │
│  - Copy .d.ts and .wasm to dist root                            │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 CLI Commands

```bash
# Full build
wasm-bodge build --crate <path>

# With explicit package.json template location
wasm-bodge build --crate <path> --package-json ./package.json

# Custom output directory (default: ./dist)
wasm-bodge build --crate <path> --out-dir ./build

# Use prebuilt wasm-bindgen output (for CI caching)
wasm-bodge build --wasm-bindgen-tar ./prebuilt.tar.gz
```

### 5.3 Phase Details

#### Phase 1: Build Wasm

```bash
# Build the Rust crate
cargo build --target wasm32-unknown-unknown --release \
  --manifest-path {crate}/Cargo.toml

# Run wasm-bindgen for each target
wasm-bindgen {wasm_file} --out-dir dist/wasm_bindgen/bundler --target bundler --weak-refs
wasm-bindgen {wasm_file} --out-dir dist/wasm_bindgen/nodejs --target nodejs --weak-refs  
wasm-bindgen {wasm_file} --out-dir dist/wasm_bindgen/web --target web --weak-refs
```

#### Phase 2: Post-Process

**Copy web to workerd:**
```bash
cp -r dist/wasm_bindgen/web dist/wasm_bindgen/workerd
```

**Apply Vite fix** to `dist/wasm_bindgen/web/{name}.js`:
```javascript
// Before:
new URL('{name}_bg.wasm', import.meta.url)
// After:  
new /* @vite-ignore */ URL('{name}_bg.wasm', import.meta.url)
```

**Generate base64 module** at `dist/esm/wasm-base64.js`:
```javascript
export const wasmBase64 = "{base64 of .wasm file}"
```

#### Phase 3: Generate Entrypoints

Write the ESM entrypoints as shown in section 4.3.

Bundle CJS versions using esbuild:
```javascript
await esbuild.build({
  entryPoints: ['dist/esm/node.js', 'dist/esm/web.js', 'dist/esm/slim.js'],
  outdir: 'dist/cjs',
  bundle: true,
  format: 'cjs',
  platform: 'node',
  packages: 'external',
  outExtension: { '.js': '.cjs' },
})
```

Bundle IIFE:
```javascript
await esbuild.build({
  entryPoints: ['dist/esm/web.js'],
  outfile: 'dist/iife/index.js',
  bundle: true,
  format: 'iife',
  globalName: '{PascalCaseName}',
})
```

#### Phase 4: Finalize Package

- Read template `package.json`
- Merge in generated `type`, `main`, `module`, `types`, `files`, `exports`
- Write to `dist/package.json`
- Copy `dist/wasm_bindgen/nodejs/{name}.d.ts` to `dist/index.d.ts`
- Copy `dist/wasm_bindgen/web/{name}_bg.wasm` to `dist/{package_name}.wasm`

---

## 6. Configuration

### 6.1 Template package.json

The only required configuration. Create a `package.json` with your package metadata:

```json
{
  "name": "my-wasm-lib",
  "version": "1.0.0", 
  "license": "MIT",
  "description": "Description of my library",
  "repository": "github:user/repo",
  "keywords": ["wasm", "rust"]
}
```

The tool will preserve all fields and add the necessary `exports`, `main`, 
`module`, `types`, and `files` fields.

### 6.2 CLI Options

| Option | Required | Default | Description |
|--------|----------|---------|-------------|
| `--crate <path>` | Yes* | - | Path to Rust crate directory |
| `--package-json <path>` | No | `./package.json` | Template package.json |
| `--out-dir <path>` | No | `./dist` | Output directory |
| `--wasm-bindgen-tar <path>` | No | - | Use prebuilt wasm-bindgen output |
| `--profile <name>` | No | `release` | Cargo build profile |

*Not required if `--wasm-bindgen-tar` is provided.

### 6.3 Optional Config File

For convenience, you can create `wasm-bodge.toml`:

```toml
crate = "../rust/my-wasm-lib"
out_dir = "./dist"
profile = "release"
```

CLI options override config file values.

---

## 7. Testing

### 7.1 Test Matrix

The tool should include a test harness to verify the generated package works
across all supported environments:

```bash
wasm-bodge test
```

| Test Case | Runtime | Module System | Entrypoint |
|-----------|---------|---------------|------------|
| webpack_cjs_fullfat | Browser (Webpack) | CJS | default |
| webpack_cjs_slim | Browser (Webpack) | CJS | slim |
| webpack_esm_fullfat | Browser (Webpack) | ESM | default |
| webpack_esm_slim | Browser (Webpack) | ESM | slim |
| node_cjs_fullfat | Node.js | CJS | default |
| node_cjs_slim | Node.js | CJS | slim |
| node_esm_fullfat | Node.js | ESM | default |
| node_esm_slim | Node.js | ESM | slim |
| vite_dev_fullfat | Browser (Vite dev) | ESM | default |
| vite_dev_slim | Browser (Vite dev) | ESM | slim |
| vite_build_fullfat | Browser (Vite build) | ESM | default |
| vite_build_slim | Browser (Vite build) | ESM | slim |
| workerd_fullfat | Cloudflare Workers | ESM | default |
| workerd_slim | Cloudflare Workers | ESM | slim |
| iife_script | Browser | Script tag | iife |

### 7.2 Test Approach

1. Run `wasm-bodge build` on a test crate
2. Run `npm pack` on the output
3. For each test case:
   - Create temporary project from template
   - Install the tarball
   - Run test (browser via Puppeteer, Node via exec, workerd via wrangler)
   - Verify expected output

### 7.3 Special Validations

**Vite single-wasm check**: After `vite build`, verify only ONE `.wasm` file
exists in `dist/assets/`. Multiple files indicate the `@vite-ignore` fix failed.

---

## Appendix: Design Rationale

### Why not publish wasm-bindgen output directly?

wasm-bindgen generates code for a single target. It doesn't provide:
- Conditional exports for different runtimes
- A "slim" option for library authors
- Base64-embedded wasm for restricted environments
- IIFE bundles for `<script>` tags

### Why copy web/ to workerd/?

Cloudflare Workers use the web-style API but require explicit `initSync()`.
Having a separate directory makes the entrypoint cleaner.

### Why base64 encoding?

Some environments (CSP-restricted pages, certain CDNs) can't fetch external
wasm files. Base64 embedding bundles wasm directly in JS at the cost of ~33%
larger payload.

### Why the @vite-ignore fix?

Vite's asset scanner finds `new URL('...wasm', import.meta.url)` patterns and
bundles the wasm file. Without `@vite-ignore`, Vite may bundle multiple copies
of the wasm file from different wasm-bindgen targets.
