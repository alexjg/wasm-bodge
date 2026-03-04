// Test that requiring the root export auto-initializes wasm for slim too.
// Both cjs/node.cjs and cjs/slim.cjs require the same cjs/web-bindings.cjs,
// and Node's require cache ensures they share state.

const { add } = require('test-wasm-lib');
const { greet } = require('test-wasm-lib/slim');

// add comes from root (auto-initialized)
const sum = add(2, 3);
if (sum !== 5) {
  throw new Error(`Expected add(2, 3) = 5, got ${sum}`);
}

// greet comes from slim (should work without manual init)
const greeting = greet('World');
if (greeting !== 'Hello, World!') {
  throw new Error(`Expected greet('World') = 'Hello, World!', got ${greeting}`);
}

console.log('WASM_BODGE_TEST_PASSED');
