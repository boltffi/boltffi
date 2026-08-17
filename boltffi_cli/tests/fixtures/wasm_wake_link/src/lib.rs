//! One async export is all it takes.
//!
//! Awaiting across the boundary reaches `boltffi_core`'s wasm waker, which
//! calls `__boltffi_wake`. That function is provided by the host at
//! instantiation, so it has to be declared as a wasm import; otherwise the
//! linker looks for a definition and does not find one.

use boltffi::export;

#[export]
pub async fn answer() -> u32 {
    42
}
