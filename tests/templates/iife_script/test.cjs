const fs = require('fs');
const vm = require('vm');

// Simulate browser globals
globalThis.atob = (b64) => Buffer.from(b64, 'base64').toString('binary');

// Read and evaluate the IIFE bundle using the package's iife export
const iifePath = require.resolve('test-wasm-lib/iife');
const iifeCode = fs.readFileSync(iifePath, 'utf-8');

// Run the IIFE code which assigns to a var
vm.runInThisContext(iifeCode);

async function main() {
  const { expectPanics } = await import('./panic-assertions.mjs');

  if (typeof TestWasmLib === 'undefined') {
    throw new Error('TestWasmLib not defined after running IIFE');
  }

  const result1 = TestWasmLib.add(2, 3);
  const result2 = TestWasmLib.greet('World');
  await expectPanics(TestWasmLib);

  if (result1 !== 5 || result2 !== 'Hello, World!') {
    throw new Error(`Test failed: ${result1}, ${result2}`);
  }

  console.log('WASM_BODGE_TEST_PASSED');
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
