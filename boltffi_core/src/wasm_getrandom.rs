//! A `getrandom` backend for `wasm32-unknown-unknown` that draws from the
//! host's Web Crypto through the runtime's `env.__boltffi_getrandom` import.

/// What `__boltffi_getrandom` returns; `@boltffi/runtime` uses the same codes.
#[cfg(target_arch = "wasm32")]
const FILLED: u32 = 0;
#[cfg(target_arch = "wasm32")]
const UNAVAILABLE: u32 = 1;

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn __boltffi_getrandom(dest: *mut u8, len: usize) -> u32;
}

/// Fills `len` bytes at `dest` with `crypto.getRandomValues`.
///
/// # Safety
///
/// `dest` must be valid for writes of `len` bytes. It may be uninitialized.
#[cfg(target_arch = "wasm32")]
pub unsafe fn fill(dest: *mut u8, len: usize) -> Result<(), getrandom::Error> {
    if len == 0 {
        return Ok(());
    }
    match unsafe { __boltffi_getrandom(dest, len) } {
        FILLED => Ok(()),
        UNAVAILABLE => Err(getrandom::Error::UNSUPPORTED),
        _ => Err(getrandom::Error::UNEXPECTED),
    }
}

/// Installs the BoltFFI runtime as `getrandom`'s entropy source on
/// `wasm32-unknown-unknown`.
///
/// `getrandom` has no default backend there, and its `wasm_js` one needs
/// `wasm-bindgen`'s glue, which a BoltFFI package does not have. This defines
/// `getrandom`'s custom backend to call `crypto.getRandomValues` instead,
/// writing straight into wasm memory. It expands to nothing on other targets.
///
/// Invoke it once, in the crate that builds the wasm module, with the
/// `getrandom` feature of `boltffi` enabled:
///
/// ```ignore
/// boltffi::wasm_getrandom_backend!();
/// ```
///
/// and select the custom backend for the wasm target, e.g. in
/// `.cargo/config.toml`:
///
/// ```toml
/// [target.wasm32-unknown-unknown]
/// rustflags = ["--cfg", 'getrandom_backend="custom"']
/// ```
///
/// This serves `getrandom` 0.4 and everything that draws from it (`rand`,
/// `uuid`'s `v4`, …).
#[macro_export]
macro_rules! wasm_getrandom_backend {
    () => {
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        #[unsafe(no_mangle)]
        unsafe extern "Rust" fn __getrandom_v03_custom(
            dest: *mut u8,
            len: usize,
        ) -> ::core::result::Result<(), $crate::__getrandom::Error> {
            unsafe { $crate::__getrandom::fill(dest, len) }
        }
    };
}
