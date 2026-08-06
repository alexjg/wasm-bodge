import {
  add,
  drop_count,
  greet,
  initSync,
  panic_async,
  panic_sync,
} from 'test-wasm-lib/slim';
import { createRequire } from 'node:module';
import { expectPanics } from './panic-assertions.mjs';

// Initialize wasm manually using the package's wasm export
const require = createRequire(import.meta.url);
const wasmPath = require.resolve('test-wasm-lib/wasm');
const wasmBytes = require('node:fs').readFileSync(wasmPath);
initSync({ module: wasmBytes });

// Run tests
const result1 = add(2, 3);
if (result1 !== 5) {
  throw new Error(`add(2, 3) expected 5, got ${result1}`);
}

const result2 = greet('World');
if (result2 !== 'Hello, World!') {
  throw new Error(`greet("World") expected "Hello, World!", got ${result2}`);
}

await expectPanics({ add, drop_count, panic_async, panic_sync });

console.log('WASM_BODGE_TEST_PASSED');
