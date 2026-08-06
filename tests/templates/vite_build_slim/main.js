import { add, drop_count, greet, initSync, panic_async, panic_sync } from 'test-wasm-lib/slim';
import wasmUrl from 'test-wasm-lib/wasm?url';
import { expectPanics } from './panic-assertions.mjs';

try {
  // Fetch and initialize wasm
  const response = await fetch(wasmUrl);
  const bytes = await response.arrayBuffer();
  initSync({ module: new Uint8Array(bytes) });

  const result1 = add(2, 3);
  const result2 = greet('World');
  await expectPanics({ add, drop_count, panic_async, panic_sync });

  document.getElementById('result').textContent =
    result1 === 5 && result2 === 'Hello, World!'
      ? 'WASM_BODGE_TEST_PASSED'
      : 'FAILED: ' + result1 + ', ' + result2;
} catch (e) {
  document.getElementById('result').textContent = 'ERROR: ' + e.message;
}
