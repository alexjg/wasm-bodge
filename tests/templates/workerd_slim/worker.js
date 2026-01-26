import { add, greet, initSync } from 'test-wasm-lib/slim';
import { wasmBase64 } from 'test-wasm-lib/wasm-base64';

// Initialize wasm from base64
const bytes = Uint8Array.from(atob(wasmBase64), (c) => c.charCodeAt(0));
initSync(bytes);

export default {
  async fetch(request) {
    const result1 = add(2, 3);
    const result2 = greet('World');

    if (result1 === 5 && result2 === 'Hello, World!') {
      return new Response('WASM_BODGE_TEST_PASSED');
    } else {
      return new Response('FAILED: ' + result1 + ', ' + result2, { status: 500 });
    }
  },
};
