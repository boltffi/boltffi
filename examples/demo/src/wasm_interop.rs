use std::sync::atomic::{AtomicU32, Ordering};

use boltffi::{CustomFfiConvertible, custom_ffi, data, export};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

static START_COUNT: AtomicU32 = AtomicU32::new(0);

pub struct JavaScriptBoolean(bool);

#[custom_ffi]
impl CustomFfiConvertible for JavaScriptBoolean {
    type FfiRepr = bool;
    type Error = String;

    fn into_ffi(&self) -> bool {
        js_sys::Boolean::from(self.0).value_of()
    }

    fn try_from_ffi(value: bool) -> Result<Self, Self::Error> {
        Ok(Self(value))
    }
}

#[export]
pub const WASM_TRUE: JavaScriptBoolean = JavaScriptBoolean(true);

#[export]
pub const WASM_FALSE: JavaScriptBoolean = JavaScriptBoolean(false);

#[wasm_bindgen]
pub enum JavaScriptState {
    Ready,
    Stopped,
}

#[wasm_bindgen(module = "/src/wasm_interop.js")]
extern "C" {
    fn increment(value: i32) -> i32;
    fn invoke(callback: &js_sys::Function, value: i32) -> i32;
    fn startup_hook();
}

#[wasm_bindgen(inline_js = "export function inline_value() { return 73; }")]
extern "C" {
    fn inline_value() -> i32;
}

#[wasm_bindgen(start)]
pub fn initialize_javascript() {
    START_COUNT.fetch_add(1, Ordering::Relaxed);
    startup_hook();
}

#[export]
pub fn wasm_uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[export]
pub fn wasm_uuid_v7() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[export]
pub fn wasm_current_time() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[export]
pub fn wasm_local_offset() -> i32 {
    chrono::Local::now().offset().local_minus_utc()
}

#[export]
pub fn wasm_json_accepts(source: String) -> bool {
    js_sys::JSON::parse(&source).is_ok()
}

#[export]
pub fn wasm_js_closure(value: i32) -> i32 {
    let callback =
        Closure::wrap(Box::new(|argument: i32| argument * 3) as Box<dyn FnMut(i32) -> i32>);
    invoke(callback.as_ref().unchecked_ref(), value)
}

#[export]
pub fn wasm_local_snippet(value: i32) -> i32 {
    increment(value)
}

#[export]
pub fn wasm_inline_snippet() -> i32 {
    inline_value()
}

#[export]
pub fn wasm_start_count() -> u32 {
    START_COUNT.load(Ordering::Relaxed)
}

#[export]
pub async fn wasm_await_promise(value: String) -> String {
    let (sender, receiver) = futures_channel::oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let resolved = JsFuture::from(js_sys::Promise::resolve(&JsValue::from_str(&value)))
            .await
            .expect("resolved promise");
        let value = resolved
            .as_string()
            .expect("promise resolves to the supplied string");
        drop(sender.send(value));
    });
    receiver.await.expect("JavaScript task completes")
}

#[export]
pub async fn wasm_async_panic() {
    wasm_await_promise("wake before panicking".to_owned()).await;
    panic!("async demo panic");
}

#[data]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum WasmUnsigned32 {
    HighBit = 0x80000000,
    Maximum = 0xffffffff,
}

#[data]
#[derive(Clone, Copy)]
#[repr(u64)]
pub enum WasmUnsigned64 {
    HighBit = 0x8000000000000000,
    Maximum = 0xffffffffffffffff,
}

#[export]
pub fn wasm_unsigned_enum32(value: WasmUnsigned32) -> WasmUnsigned32 {
    value
}

#[export]
pub fn wasm_unsigned_enum64(value: WasmUnsigned64) -> WasmUnsigned64 {
    value
}

#[export]
pub trait WasmUnsignedCallback {
    fn matches(&self, word: u32, wide: u64, size: usize) -> bool;
    fn echo_enums(&self, word: WasmUnsigned32, wide: WasmUnsigned64) -> WasmUnsigned64;
}

#[export]
pub fn wasm_unsigned_callback(
    callback: impl WasmUnsignedCallback,
    word: u32,
    wide: u64,
    size: usize,
) -> bool {
    callback.matches(word, wide, size)
}

#[export]
pub fn wasm_unsigned_enum_callback(
    callback: impl WasmUnsignedCallback,
    word: WasmUnsigned32,
    wide: WasmUnsigned64,
) -> WasmUnsigned64 {
    callback.echo_enums(word, wide)
}

#[export]
pub fn wasm_unsigned_closure(
    callback: impl Fn(u32, u64, usize) -> bool,
    word: u32,
    wide: u64,
    size: usize,
) -> bool {
    callback(word, wide, size)
}

#[export]
pub fn wasm_unsigned_enum_closure(
    callback: impl Fn(WasmUnsigned32, WasmUnsigned64) -> WasmUnsigned64,
    word: WasmUnsigned32,
    wide: WasmUnsigned64,
) -> WasmUnsigned64 {
    callback(word, wide)
}

pub struct WasmUnsignedValue(u64);

#[export]
impl WasmUnsignedValue {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }

    pub async fn async_value(&self) -> u64 {
        self.0
    }
}
