import { add, greet, default as init } from 'test-wasm-lib/slim';

async function run() {
  try {
    await init();
    const result1 = add(2, 3);
    const result2 = greet('World');

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
