// Test that requiring the root export auto-initializes wasm for slim too.
// Both cjs/node.cjs and cjs/slim.cjs require the same cjs/web-bindings.cjs,
// and Node's require cache ensures they share state.

const { add } = require('test-wasm-lib');
const { drop_count, greet, panic_async, panic_sync } = require('test-wasm-lib/slim');

async function main() {
  const { expectPanics } = await import('./panic-assertions.mjs');

  // add comes from root (auto-initialized)
  const sum = add(2, 3);
  if (sum !== 5) {
    throw new Error(`Expected add(2, 3) = 5, got ${sum}`);
  }

  // greet and panic functions come from slim (and should work without manual init)
  const greeting = greet('World');
  if (greeting !== 'Hello, World!') {
    throw new Error(`Expected greet('World') = 'Hello, World!', got ${greeting}`);
  }

  await expectPanics({ add, drop_count, panic_async, panic_sync });
  console.log('WASM_BODGE_TEST_PASSED');
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
