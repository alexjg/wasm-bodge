async function expectPanic({ add, drop_count }, label, expectedMessage, invoke) {
  const dropsBefore = drop_count();
  let error;
  try {
    await invoke();
  } catch (caught) {
    error = caught;
  }

  if (!(error instanceof Error)) throw new Error(`${label} did not throw an Error`);
  if (error.name !== 'PanicError') {
    throw new Error(`${label} threw ${error.name}, not PanicError`);
  }
  if (error.message !== expectedMessage) {
    throw new Error(`${label} message expected ${expectedMessage}, got ${error.message}`);
  }
  if (drop_count() !== dropsBefore + 1) {
    throw new Error(`${label} did not run Rust destructors`);
  }
  if (add(20, 22) !== 42) {
    throw new Error(`Wasm instance was poisoned after ${label}`);
  }
}

export async function expectPanics(bindings) {
  await expectPanic(bindings, 'sync panic', 'expected sync panic', () => bindings.panic_sync());
  await expectPanic(bindings, 'async panic', 'expected async panic', () => bindings.panic_async());
}
