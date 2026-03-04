// Test that importing the root export auto-initializes wasm for slim too.
// The root export initializes via initSync, and since both root and slim
// import from the same underlying web target module, the slim export should
// also be functional without manual initialization.

import { add } from 'test-wasm-lib';
import { greet } from 'test-wasm-lib/slim';

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
