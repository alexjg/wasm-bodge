import init, {
  add,
  drop_count,
  greet,
  panic_async,
  panic_sync,
} from 'test-wasm-lib/debug/slim';
import wasmUrl from 'test-wasm-lib/debug/wasm?url';
import { expectPanics } from './panic-assertions.mjs';

try {
  // Fetch and initialize wasm
  const response = await fetch(wasmUrl);
  const bytes = await response.arrayBuffer();
  // Unwind-enabled debug builds include an unoptimized std and can exceed
  // Chromium's 8 MB synchronous compilation and instantiation limits.
  await init({ module_or_path: bytes });

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
