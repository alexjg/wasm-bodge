const fs = require('fs');
const vm = require('vm');

// Simulate browser globals
globalThis.atob = (b64) => Buffer.from(b64, 'base64').toString('binary');

// Read and evaluate the IIFE bundle using the package's iife export
const iifePath = require.resolve('test-wasm-lib/iife');
const iifeCode = fs.readFileSync(iifePath, 'utf-8');

// Run the IIFE code which assigns to a var
vm.runInThisContext(iifeCode);

// Now TestWasmLib should be defined globally
if (typeof TestWasmLib === 'undefined') {
  console.error('TestWasmLib not defined after running IIFE');
  process.exit(1);
}

// Test the functions
const result1 = TestWasmLib.add(2, 3);
const result2 = TestWasmLib.greet('World');

if (result1 === 5 && result2 === 'Hello, World!') {
  console.log('WASM_BODGE_TEST_PASSED');
} else {
  console.error('Test failed:', result1, result2);
  process.exit(1);
}
