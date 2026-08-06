import {
  add,
  CallbackDriver,
  drop_count,
  greet,
  panic_async,
  panic_sync,
  WrappedValue,
} from 'test-wasm-lib/debug';
import { WrappedValue as SlimWrappedValue } from 'test-wasm-lib/debug/slim';
import { expectPanics } from './panic-assertions.mjs';

async function run() {
  try {
    if (WrappedValue !== SlimWrappedValue) {
      throw new Error('debug root and slim wrapper constructors differ');
    }

    let received;
    new CallbackDriver({
      receive(value) {
        if (!(value instanceof WrappedValue)) {
          throw new Error('callback received a wrapper from another debug glue module');
        }
        received = value.value;
      },
    }).deliver();

    const result1 = add(2, 3);
    const result2 = greet('Debug');
    await expectPanics({ add, drop_count, panic_async, panic_sync });
    document.getElementById('result').textContent =
      result1 === 5 && result2 === 'Hello, Debug!' && received === 42
        ? 'WASM_BODGE_TEST_PASSED'
        : 'FAILED: ' + result1 + ', ' + result2 + ', ' + received;
  } catch (e) {
    document.getElementById('result').textContent = 'ERROR: ' + e.message;
  }
}

run();
