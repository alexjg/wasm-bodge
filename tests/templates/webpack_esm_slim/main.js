import {
  add,
  default as init,
  drop_count,
  greet,
  panic_async,
  panic_sync,
} from 'test-wasm-lib/slim';
import { expectPanics } from './panic-assertions.mjs';

async function run() {
  try {
    await init();
    const result1 = add(2, 3);
    const result2 = greet('World');
    await expectPanics({ add, drop_count, panic_async, panic_sync });

    if (result1 === 5 && result2 === 'Hello, World!') {
      document.getElementById('result').textContent = 'WASM_BODGE_TEST_PASSED';
    } else {
      document.getElementById('result').textContent = 'FAILED: ' + result1 + ', ' + result2;
    }
  } catch (e) {
    document.getElementById('result').textContent = 'ERROR: ' + e.message;
  }
}

run();
