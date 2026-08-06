use std::sync::atomic::{AtomicU32, Ordering};
use wasm_bindgen::prelude::*;

static DROP_COUNT: AtomicU32 = AtomicU32::new(0);

struct DropGuard;

impl Drop for DropGuard {
    fn drop(&mut self) {
        DROP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

/// Add two numbers together
#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Greet someone by name
#[wasm_bindgen]
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

/// Panic after creating a value with an observable destructor.
#[wasm_bindgen]
pub fn panic_sync() {
    let _guard = DropGuard;
    panic!("expected sync panic");
}

/// Asynchronously panic after creating a value with an observable destructor.
#[wasm_bindgen]
pub async fn panic_async() {
    let _guard = DropGuard;
    panic!("expected async panic");
}

/// Number of panic guards that have been dropped while unwinding.
#[wasm_bindgen]
pub fn drop_count() -> u32 {
    DROP_COUNT.load(Ordering::SeqCst)
}

/// A wasm-bindgen wrapper used to verify JavaScript constructor identity.
#[wasm_bindgen]
pub struct WrappedValue {
    value: u32,
}

#[wasm_bindgen]
impl WrappedValue {
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> u32 {
        self.value
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "any")]
    pub type ValueSink;

    #[wasm_bindgen(method, js_name = receive)]
    fn receive(this: &ValueSink, value: WrappedValue);
}

/// Calls a JavaScript sink with a Rust-created wrapper value.
#[wasm_bindgen]
pub struct CallbackDriver {
    sink: ValueSink,
}

#[wasm_bindgen]
impl CallbackDriver {
    #[wasm_bindgen(constructor)]
    pub fn new(sink: ValueSink) -> Self {
        Self { sink }
    }

    pub fn deliver(&self) {
        self.sink.receive(WrappedValue { value: 42 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
        assert_eq!(add(-1, 1), 0);
        assert_eq!(add(0, 0), 0);
    }

    #[test]
    fn test_greet() {
        assert_eq!(greet("World"), "Hello, World!");
        assert_eq!(greet("Rust"), "Hello, Rust!");
    }
}
