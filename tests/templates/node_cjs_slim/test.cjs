const { add, greet, initSync } = require('test-wasm-lib/slim');
const fs = require('fs');

// Initialize wasm manually using the package's wasm export
const wasmPath = require.resolve('test-wasm-lib/wasm');
const wasmBytes = fs.readFileSync(wasmPath);
initSync(wasmBytes);

// Run tests
const result1 = add(2, 3);
if (result1 !== 5) {
  throw new Error(`add(2, 3) expected 5, got ${result1}`);
}

const result2 = greet('World');
if (result2 !== 'Hello, World!') {
  throw new Error(`greet("World") expected "Hello, World!", got ${result2}`);
}

console.log('WASM_BODGE_TEST_PASSED');
