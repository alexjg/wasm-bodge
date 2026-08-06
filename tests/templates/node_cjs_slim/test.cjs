const { add, drop_count, greet, initSync, panic_async, panic_sync } = require('test-wasm-lib/slim');
const fs = require('fs');

async function main() {
  const { expectPanics } = await import('./panic-assertions.mjs');

  // Initialize wasm manually using the package's wasm export
  const wasmPath = require.resolve('test-wasm-lib/wasm');
  const wasmBytes = fs.readFileSync(wasmPath);
  initSync({ module: wasmBytes });

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
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
