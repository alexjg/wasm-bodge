import { add, greet } from 'test-wasm-lib';

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
