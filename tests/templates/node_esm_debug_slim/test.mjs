// End-to-end regression test for the `/debug/slim` + `/debug/wasm` pairing.
//
// On wasm-bodge <=0.2.3, the "obvious" way to manually initialize the debug
// wasm -- combine `/slim` (manual-init JS) with `/debug/wasm` (debug binary)
// -- crashed at the first call into the module with
// `TypeError: wasm.__wbindgen_export3 is not a function`. The cause: wasm-opt
// renames wasm exports in the optimized variant, so `/slim`'s JS bindings
// are pinned to the optimized ABI and cannot drive the debug wasm. This
// test loads the debug wasm through `/debug/slim` (which re-exports the
// debug variant's JS bindings) and calls into it, proving the matched-pair
// combination works.
import { add, greet, initSync } from 'test-wasm-lib/debug/slim';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const wasmPath = require.resolve('test-wasm-lib/debug/wasm');
const wasmBytes = require('node:fs').readFileSync(wasmPath);
initSync(wasmBytes);

const result1 = add(2, 3);
if (result1 !== 5) {
  throw new Error(`add(2, 3) expected 5, got ${result1}`);
}

const result2 = greet('World');
if (result2 !== 'Hello, World!') {
  throw new Error(`greet("World") expected "Hello, World!", got ${result2}`);
}

console.log('WASM_BODGE_TEST_PASSED');
