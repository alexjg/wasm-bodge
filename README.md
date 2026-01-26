# wasm-bodge

[![CI](https://github.com/alexjg/wasm-bodge/actions/workflows/ci.yml/badge.svg)](https://github.com/alexjg/wasm-bodge/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/wasm-bodge.svg)](https://crates.io/crates/wasm-bodge)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

A CLI tool for taking wasm-bindgen based Wasm libraries and building an NPM package that works everywhere (well, lots of places).

> **Note:** The problem this tool is solving is tricky, and the things we do to solve it are complicated and somewhat fragile. It's best to have a full understanding of what exactly is being done in order to be able to debug things effectively. Please read the full README.

## TL;DR

**What it does:** Takes a Rust crate using `wasm-bindgen` and produces a universal NPM package.

Basically you do this inside your rust crate:

```bash
wasm-bodge build
```

And now you have a ready to publish NPM package.

**Supported environments:**
- Node.js (ESM and CommonJS)
- Browsers (with bundlers like Webpack, Vite, Rollup)
- Browsers (without bundlers, via base64-embedded wasm)
- Cloudflare Workers (workerd)
- Script tags (IIFE)

**Key exports:**

The package produced by `wasm-bodge` provides the following subpath exports which help with handling WebAssembly initialization in different environments:

| Export | Description |
|--------|-------------|
| `.` | Auto-detected environment entry point |
| `./slim` | Manual initialization (for library authors) |
| `./wasm` | Raw `.wasm` file |
| `./wasm-base64` | Base64-encoded wasm |
| `./iife` | IIFE bundle for `<script>` tags |

---

## Table of Contents

- [Quickstart](#quickstart)
- [CLI Reference](#cli-reference)
- [The Problem](#the-problem)
- [How wasm-bodge Solves It](#how-wasm-bodge-solves-it)
  - [Subpath Exports](#subpath-exports)
  - [Environment-Specific Strategies](#environment-specific-strategies)
  - [The `/slim` Escape Hatch](#the-slim-escape-hatch)
- [Build Output](#build-output)
- [Technical Details](#technical-details)
  - [WebAssembly Initialization](#webassembly-initialization)
  - [Fixing Vite's Asset Preprocessor](#fixing-vites-asset-preprocessor)
- [Troubleshooting](#troubleshooting)

---

## Quickstart

```bash
# Prerequisites: Rust with wasm32-unknown-unknown target, wasm-bindgen-cli, esbuild

# Build your wasm crate
wasm-bodge build

# Publish from the directory containing package.json
npm publish
```

Your users can then import it anywhere:

```javascript
import { myFunction } from "my-wasm-lib"
```

---

## CLI Reference

```
wasm-bodge build [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--crate-path <PATH>` | `.` (current dir) | Path to the Rust crate directory |
| `--package-json <PATH>` | `./package.json` | Path to template package.json |
| `--out-dir <PATH>` | `./dist` | Output directory for generated files |
| `--profile <PROFILE>` | `release` | Cargo build profile |
| `--wasm-bindgen-tar <PATH>` | (none) | Use prebuilt wasm-bindgen output from tarball |

**Prerequisites:**
- Rust with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- `wasm-bindgen-cli` (`cargo install wasm-bindgen-cli`)
- `esbuild` (`npm install -g esbuild` or local install)

---

## The Problem

The output of `wasm-bindgen` is not a JavaScript package that can be loaded in any environment.

Writing WebAssembly libraries in Rust is generally achieved using the `wasm-bindgen` toolchain. Using `wasm-bindgen` involves three steps:

1. Write code in Rust annotated with the `wasm-bindgen` macros
2. Compile the Rust code to WebAssembly using `cargo build --target wasm32-unknown-unknown`
3. Use the `wasm-bindgen` CLI tool to generate JavaScript and WebAssembly files from the compiled Wasm

Here's an example. Say we have a Rust crate called `my-rust-crate` with this code:

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(left: u32, right: u32) -> u32 {
    left + right
}
```

To build this for Node.js we first compile it for the wasm32-unknown-unknown target:

```bash
cargo build --target wasm32-unknown-unknown --release
```

Then, we use `wasm-bindgen` to generate the JS and Wasm files:

```bash
wasm-bindgen --target nodejs --out-dir ./out ./target/wasm32-unknown-unknown/release/my_rust_crate.wasm
```

This generates output in `./out`:

```
out/
  my_rust_crate.d.ts
  my_rust_crate.js
  my_rust_crate_bg.wasm
  my_rust_crate_bg.wasm.d.ts
```

The `.d.ts` files are TypeScript declarations. The `.js` file contains JavaScript glue code that provides a nice interface to the WebAssembly module. We can import it like so:

```javascript
import { add } from "./out/my_rust_crate.js";
add(1, 2);
```

To make this into a package, we create a `package.json`:

```json
{
  "name": "my-wasm-lib",
  "version": "1.0.0",
  "main": "out/my_rust_crate.js",
  "exports": {
    ".": "./out/my_rust_crate.js"
  }
}
```

This works in Node.js:

```javascript
import { add } from "my-wasm-lib";
add(1, 2);
```

**But it fails in Deno:**

```
> deno
Deno 2.6.5
> let { add } = await import("my-wasm-lib")
Uncaught ReferenceError: exports is not defined
    at file:///tmp/wat/my-rust-crate/dist/my_rust_crate.js:12:1
```

The problem is that `--target nodejs` produces CommonJS modules, but Deno only supports ES Modules. We could use `--target deno`, but then Node.js wouldn't work.

**This is the core problem wasm-bodge solves.**

---

## How wasm-bodge Solves It

The solution is to:

1. Bundle all the different wasm-bindgen strategies into the package
2. Use subpath exports to choose the right strategy for the current environment
3. Provide an escape hatch for environments where detection doesn't work

### Subpath Exports

Subpath exports are a `package.json` feature that allows different entry points for different environments:

```json
{
  "exports": {
    "import": "./esm/index.js",
    "require": "./cjs/index.js"
  }
}
```

This tells the module resolver to use `./esm/index.js` for ES Module `import` statements, and `./cjs/index.js` for CommonJS `require` calls.

We can be more specific with "conditional exports":

```json
{
  "exports": {
    "node": {
      "import": "./esm/node.js",
      "require": "./cjs/node.js"
    },
    "browser": "./esm/browser.js"
  }
}
```

The module resolver picks the most specific match for the current environment. Which conditions are supported depends on the environment and is generally not well standardized or documented - that's why this approach is fragile and why we need an escape hatch.

### Environment-Specific Strategies

`wasm-bodge` creates entrypoint scripts for each supported environment, then uses `esbuild` to bundle them with the appropriate `wasm-bindgen` output.

---

#### Node.js

`wasm-bindgen --target nodejs` produces CommonJS modules. Attempting to import this in our entry point scripts will throw errors because our package uses `"type": "module"` which means Node will attempt to import any `.js` file as an ES Module. To solve this we rename the generated `.js` to `.cjs` so Node.js treats it correctly.

**ES Module Entrypoint** (`./dist/esm/node.js`):
```javascript
export * from '../wasm_bindgen/nodejs/<lib name>.cjs';
```

**CommonJS Entrypoint** (`./dist/cjs/node.cjs`):
```javascript
module.exports = require('../wasm_bindgen/nodejs/<lib name>.cjs');
```

---

#### Browsers (without bundler)

Browsers don't support importing `.wasm` directly, and we don't know what URL the wasm will be served from. so we embed the wasm as base64 in the JS file.

We use `--target web` and add a build step that base64-encodes the `.wasm` file into `wasm-base64.js`.

**ES Module Entrypoint** (`./dist/esm/web.js`):
```javascript
import { initSync } from '../wasm_bindgen/web/<lib name>.js';
import { wasmBase64 } from './wasm-base64.js';
const bytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0));
initSync(bytes);
export * from '../wasm_bindgen/web/<lib name>.js';
```

**CommonJS Entrypoint** (`./dist/cjs/web.cjs`):
Bundled from the ESM entrypoint using `esbuild --format=cjs`.

---

#### Bundlers (Webpack, Vite, Rollup, etc.)

Bundlers can use `--target bundler` directly since they handle `.wasm` imports.

**ES Module Entrypoint** (`./dist/esm/bundler.js`):
```javascript
export * from '../wasm_bindgen/bundler/<lib name>.js';
```

**CommonJS Entrypoint** (`./dist/cjs/bundler.cjs`):
Falls back to the base64 web entrypoint since CommonJS can't import `.wasm` directly.

---

#### Cloudflare Workers (workerd)

Cloudflare Workers allow synchronous `.wasm` imports but still need JS wrapper initialization.

**ES Module Entrypoint** (`./dist/esm/workerd.js`):
```javascript
import * as exports from '../wasm_bindgen/web/<lib name>.js';
import { initSync } from '../wasm_bindgen/web/<lib name>.js';
import wasmModule from '../wasm_bindgen/web/<lib name>_bg.wasm';
initSync({ module: wasmModule });
export * from '../wasm_bindgen/web/<lib name>.js';
```

**CommonJS Entrypoint** (`./dist/cjs/workerd.cjs`):
Falls back to the base64 web entrypoint.

---

#### IIFE (Script Tags)

For `<script>` tag usage in browsers, we bundle the web entrypoint as an IIFE:

```bash
esbuild ./dist/esm/web.js --bundle --format=iife --global-name=MyWasmLib
```

Usage:
```html
<script src="path/to/my-wasm-lib/dist/iife/index.js"></script>
<script>
  MyWasmLib.myFunction();
</script>
```

---

### The `/slim` Escape Hatch

Despite our best efforts, some environments won't work with automatic detection. The `/slim` export provides manual initialization:

```javascript
import { initSync, myFunction } from "my-wasm-lib/slim";
import wasmBytes from "my-wasm-lib/wasm";
const bytes = /* fetch or read wasmBytes as appropriate */;
initSync(bytes);
// Now use the exports
myFunction();
```

**This is crucial for library authors.** If you're writing a library that depends on a wasm-bodge package, always use the `/slim` export. This lets the application developer control WebAssembly initialization.

**ES Module Entrypoint** (`./dist/esm/slim.js`):
```javascript
export * from '../wasm_bindgen/web/<lib name>.js';
export { default } from '../wasm_bindgen/web/<lib name>.js';
```

The `web` target doesn't auto-initialize and exports `initSync` for manual initialization.

---

## Build Output

`wasm-bodge` outputs a `./dist` directory with this structure:

```
dist/
    esm/
        node.js           # Node.js ESM
        web.js            # Browser (base64 embedded)
        bundler.js        # Bundler entry
        workerd.js        # Cloudflare Workers
        slim.js           # Manual initialization
        wasm-base64.js    # Base64-encoded wasm
    cjs/
        node.cjs          # Node.js CommonJS
        web.cjs           # Browser CommonJS
        bundler.cjs       # Bundler CommonJS
        workerd.cjs       # Cloudflare CommonJS
        slim.cjs          # Manual init CommonJS
        wasm-base64.cjs   # Base64 CommonJS
    iife/
        index.js          # IIFE bundle for <script> tags
    wasm_bindgen/
        nodejs/           # wasm-bindgen --target nodejs
        web/              # wasm-bindgen --target web
        bundler/          # wasm-bindgen --target bundler
    index.d.ts            # TypeScript declarations
    <package-name>.wasm   # Raw wasm file
```

The `package.json` exports are configured as:

```json
{
  "exports": {
    ".": {
      "types": "./dist/index.d.ts",
      "workerd": {
        "import": "./dist/esm/workerd.js",
        "require": "./dist/cjs/web.cjs"
      },
      "node": {
        "import": "./dist/esm/node.js",
        "require": "./dist/cjs/node.cjs"
      },
      "browser": {
        "import": "./dist/esm/bundler.js",
        "require": "./dist/cjs/web.cjs"
      },
      "import": "./dist/esm/web.js",
      "require": "./dist/cjs/web.cjs"
    },
    "./slim": {
      "types": "./dist/index.d.ts",
      "import": "./dist/esm/slim.js",
      "require": "./dist/cjs/slim.cjs"
    },
    "./wasm": "./dist/<package-name>.wasm",
    "./wasm-base64": {
      "import": "./dist/esm/wasm-base64.js",
      "require": "./dist/cjs/wasm-base64.cjs"
    },
    "./iife": "./dist/iife/index.js"
  }
}
```

---

## Technical Details

### WebAssembly Initialization

Why does `wasm-bindgen` have different targets? They all handle WebAssembly initialization differently.

The standard WebAssembly API is imperative:

```javascript
const wasmBytes = await fetch("my_module.wasm").then(res => res.arrayBuffer());
const wasmModule = await WebAssembly.instantiate(wasmBytes, importObject);
```

We want users to import our library like any other JS module, so we need to hide this initialization. Here's what `--target nodejs` generates:

```javascript
function add(left, right) {
    const ret = wasm.add(left, right);
    return ret >>> 0;
}
exports.add = add;

const wasmPath = `${__dirname}/my_rust_crate_bg.wasm`;
const wasmBytes = require('fs').readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(wasmBytes);
const wasm = new WebAssembly.Instance(wasmModule, __wbg_get_imports()).exports;
wasm.__wbindgen_start();
```

This uses `require('fs').readFileSync(...)` which only works in Node.js CommonJS context - not in Deno, browsers, or Node.js ESM.

### Fixing Vite's Asset Preprocessor

Vite's asset scanner looks for patterns like:

```javascript
new URL("./<path>", import.meta.url)
```

The `--target web` output contains this pattern, which can cause Vite to bundle multiple copies of the wasm file.

We add a `/* @vite-ignore */` comment (undocumented usage, may break) inside the expression:

```javascript
new /* @vite-ignore */ URL('./my_lib_bg.wasm', import.meta.url);
```

The comment must be inside the `new URL(...)` because Vite:
1. Uses a regex to match `new URL(...)` patterns
2. Searches each match for `/* @vite-ignore */`
