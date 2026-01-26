# Hacking on wasm-bodge

This document explains how wasm-bodge is tested and how to modify or debug tests.


## Testing

### Testing Philosophy

wasm-bodge generates npm packages that must work across many JavaScript runtime environments (Node.js, browsers, Cloudflare Workers) and module systems (ESM, CommonJS, IIFE). Each environment has different capabilities and constraints around loading WebAssembly modules.

Rather than unit testing individual build phases, we bias heavily towards **integration tests** that exercise the entire pipeline: build the wasm, generate the package, install it in a real project, and run it in the target environment. This catches issues that only manifest when all the pieces come together—import resolution, wasm instantiation, bundler transformations, etc.

The test matrix covers:
- **Entrypoints**: fullfat (auto-init) vs slim (manual init)
- **Environments**: Node.js, Webpack, Vite, Cloudflare Workers, browser `<script>` tags
- **Module systems**: ESM, CommonJS, IIFE

### Test Structure

```
tests/
├── packaging.rs           # Integration test runner (includes Rust HTTP server)
├── puppeteer_runner/
│   ├── package.json       # Puppeteer dependency
│   └── check.mjs          # Browser verification script
├── fixtures/
│   └── test-crate/        # Minimal wasm-bindgen Rust crate
└── templates/             # Self-contained test projects
    ├── node_esm_fullfat/
    ├── node_esm_slim/
    ├── webpack_esm_fullfat/
    ├── vite_build_fullfat/
    ├── workerd_fullfat/
    └── ...
```

### How Tests Work

Each test:

1. Builds `tests/fixtures/test-crate` using wasm-bodge (once, shared across all tests)
2. Copies a template directory to a temp location
3. Installs the built package via `npm pack` + `npm install`
4. Runs `npm install` if the template has `devDependencies`
5. Runs `npm run build`
6. Runs verification:
   - **Node tests**: Run `npm test` (executes Node.js test script)
   - **Browser tests** (webpack, vite, iife): Start a server and verify with Puppeteer
   - **Workerd tests**: Run `npm test` (build success = pass)

### Browser Testing

Browser-based tests (webpack, vite, iife) use Puppeteer to verify the code actually works in a real browser:

```
tests/
├── packaging.rs              # Contains Rust HTTP server for static files
└── puppeteer_runner/
    ├── package.json          # Puppeteer dependency
    └── check.mjs             # Minimal script: opens URL, checks for #result element
```

The test harness:
1. Starts a server (Rust `tiny_http` for static files, or vite's dev/preview server)
2. Runs `check.mjs` which opens the page in headless Chrome
3. Waits for an element `#result` to contain `WASM_BODGE_TEST_PASSED`

Each browser template includes an `index.html` that loads the bundled JS and writes the test result to `#result`.

### Template Convention

Every template is a self-contained npm project with a `build` script:

```json
{
  "scripts": {
    "build": "..."
  }
}
```

| Template Type | `build` | Verification |
|---------------|---------|--------------|
| node_* | `true` (no-op) | `npm test` runs `node test.{mjs,cjs}` |
| webpack_* | `webpack --mode production` | Puppeteer checks browser |
| vite_dev_* | `true` (no-op) | Puppeteer checks vite dev server |
| vite_build_* | `vite build` | Rust checks single .wasm file, then Puppeteer checks vite preview |
| workerd_* | `wrangler deploy --dry-run --outdir dist` | Build success = pass |
| iife_script | `true` (no-op) | Puppeteer checks static server |

### Running Tests

```bash
# Run all tests
cargo test --release

# Run a specific test
cargo test --release node_esm

# Run with output visible
cargo test --release -- --nocapture
```

### Debugging a Failing Test

Templates are designed to be debuggable standalone:

```bash
# 1. Build the test package (this modifies package.json in place)
cargo run --release -- build \
  --crate-path tests/fixtures/test-crate \
  --package-json tests/fixtures/test-crate/package.json \
  --out-dir tests/fixtures/test-crate/dist

# 2. Copy the template you want to debug
cp -r tests/templates/node_esm_fullfat /tmp/debug-test
cd /tmp/debug-test

# 3. Pack and install the package (from the directory with package.json)
npm pack ~/project/tests/fixtures/test-crate
npm install test-wasm-lib-*.tgz

# 4. Install dev dependencies (if any)
npm install

# 5. Run the test manually
npm run build
npm test
```

For browser tests (webpack, vite, iife), after step 4:

```bash
# For webpack: serve the dist/ directory and open in browser
npm run build
npx serve dist

# For vite dev: run the dev server
npx vite

# For vite build: build and preview
npm run build
npx vite preview
```

Then open the URL in your browser and check the developer console.

Note: The test runner automatically restores the original `package.json` before each test run, so you don't need to worry about the in-place modifications.

### Adding a New Test

1. Create a new directory under `tests/templates/` (e.g., `my_new_test/`)

2. Add a `package.json` with a build script:
   ```json
   {
     "name": "my-new-test",
     "private": true,
     "type": "module",
     "scripts": {
       "build": "..."
     }
   }
   ```

3. Add test files:
   - For Node tests: add `test.mjs` or `test.cjs` and a `"test"` script
   - For browser tests: add `index.html` and `main.js` that write to `#result`

4. Add a test function in `tests/packaging.rs`:
   ```rust
   #[test]
   fn test_my_new_test() {
       run_test("my_new_test").unwrap();
   }
   ```

5. If it's a browser test, update `browser_test_kind()` in `packaging.rs` to recognize your template name pattern.

## Test Fixture Crate

`tests/fixtures/test-crate/` is a minimal Rust crate that exports two functions:

```rust
#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[wasm_bindgen]
pub fn greet(name: &str) -> String { format!("Hello, {}!", name) }
```

Tests verify these functions work correctly after the wasm-bodge build pipeline.

### Fullfat vs Slim

Each environment is tested in two variants:

- **Fullfat**: Uses the auto-initializing entrypoint (wasm is loaded automatically)
- **Slim**: Uses the manual initialization entrypoint (caller provides wasm bytes)

This ensures both usage patterns work across all environments.
