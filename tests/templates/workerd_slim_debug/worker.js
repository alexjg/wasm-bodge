import {
  add,
  drop_count,
  greet,
  initSync,
  panic_async,
  panic_sync,
} from 'test-wasm-lib/debug/slim';
import wasmModule from './test-wasm-lib-debug.wasm';
import { expectPanics } from './panic-assertions.mjs';

// Workers require Wasm to be compiled from a static module import.
initSync({ module: wasmModule });

export default {
  async fetch(request) {
    const result1 = add(2, 3);
    const result2 = greet('World');
    await expectPanics({ add, drop_count, panic_async, panic_sync });

    if (result1 === 5 && result2 === 'Hello, World!') {
      return new Response('WASM_BODGE_TEST_PASSED');
    } else {
      return new Response('FAILED: ' + result1 + ', ' + result2, { status: 500 });
    }
  },
};
