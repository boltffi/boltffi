//! Draws from `getrandom` through the BoltFFI runtime's Web Crypto import.
//!
//! The custom backend is selected by the test, through `RUSTFLAGS`; a real
//! crate would put `--cfg getrandom_backend="custom"` in `.cargo/config.toml`.

use boltffi::export;

boltffi::wasm_getrandom_backend!();

#[export]
pub fn random_u64() -> u64 {
    getrandom::u64().unwrap_or(0)
}
