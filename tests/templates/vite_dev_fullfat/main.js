import { add, greet, CallbackDriver, WrappedValue } from 'test-wasm-lib';
import { WrappedValue as SlimWrappedValue } from 'test-wasm-lib/slim';

try {
  if (WrappedValue !== SlimWrappedValue) {
    throw new Error('root and slim wrapper constructors differ');
  }

  let received;
  new CallbackDriver({
    receive(value) {
      if (!(value instanceof WrappedValue)) {
        throw new Error('callback received a wrapper from another glue module');
      }
      received = value.value;
    },
  }).deliver();

  const result1 = add(2, 3);
  const result2 = greet('World');

  document.getElementById('result').textContent =
    result1 === 5 && result2 === 'Hello, World!' && received === 42
      ? 'WASM_BODGE_TEST_PASSED'
      : 'FAILED: ' + result1 + ', ' + result2 + ', ' + received;
} catch (e) {
  document.getElementById('result').textContent = 'ERROR: ' + e.message;
}
