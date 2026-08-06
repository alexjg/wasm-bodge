import { add, drop_count, greet, panic_async, panic_sync } from 'test-wasm-lib';
import { expectPanics } from './panic-assertions.mjs';

const result1 = add(2, 3);
if (result1 !== 5) {
  throw new Error(`add(2, 3) expected 5, got ${result1}`);
}

const result2 = greet('World');
if (result2 !== 'Hello, World!') {
  throw new Error(`greet("World") expected "Hello, World!", got ${result2}`);
}

await expectPanics({ add, drop_count, panic_async, panic_sync });

console.log('WASM_BODGE_TEST_PASSED');
