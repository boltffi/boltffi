//! Symbol names for Dart dual-path stubs. The stubs themselves are emitted
//! by `boltffi_macros` into the user crate (`cfg(boltffi_dart)`), not as
//! generated Rust source from this backend.
//!
//! Prefixes are derived from the C `register` symbol
//! (`boltffi_register_callback_<path>`) so module/crate identity is preserved.

const REGISTER_PREFIX: &str = "boltffi_register_callback_";

pub(crate) fn shim_prefix(register_symbol: &str) -> String {
    let path = register_symbol
        .strip_prefix(REGISTER_PREFIX)
        .unwrap_or(register_symbol);
    format!("BoltFFIDartShim_{path}")
}

pub(crate) fn method_symbol(register_symbol: &str, method: &str) -> String {
    format!("{}_{method}", shim_prefix(register_symbol))
}

pub(crate) fn register_symbol(register_symbol: &str) -> String {
    format!("{}_register", shim_prefix(register_symbol))
}

pub(crate) fn release_symbol(register_symbol: &str) -> String {
    format!("{}_release", shim_prefix(register_symbol))
}
