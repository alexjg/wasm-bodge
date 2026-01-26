import { add, greet } from 'test-wasm-lib';

try {
  const result1 = add(2, 3);
  const result2 = greet('World');

  document.getElementById('result').textContent =
    result1 === 5 && result2 === 'Hello, World!'
      ? 'WASM_BODGE_TEST_PASSED'
      : 'FAILED: ' + result1 + ', ' + result2;
} catch (e) {
  document.getElementById('result').textContent = 'ERROR: ' + e.message;
}
