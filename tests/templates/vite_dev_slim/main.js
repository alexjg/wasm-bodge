import { add, greet, initSync } from 'test-wasm-lib/slim';
import wasmUrl from 'test-wasm-lib/wasm?url';

try {
  // Fetch and initialize wasm
  const response = await fetch(wasmUrl);
  const bytes = await response.arrayBuffer();
  initSync(new Uint8Array(bytes));

  const result1 = add(2, 3);
  const result2 = greet('World');

  document.getElementById('result').textContent =
    result1 === 5 && result2 === 'Hello, World!'
      ? 'WASM_BODGE_TEST_PASSED'
      : 'FAILED: ' + result1 + ', ' + result2;
} catch (e) {
  document.getElementById('result').textContent = 'ERROR: ' + e.message;
}
