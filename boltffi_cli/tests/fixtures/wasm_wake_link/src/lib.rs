//! One async export is all it takes.
//!
//! Awaiting across the boundary reaches `boltffi_core`'s wasm waker, which
//! calls `__boltffi_wake`. That function is provided by the host at
//! instantiation, so it has to be declared as a wasm import; otherwise the
//! linker looks for a definition and does not find one.

boltffi::scaffolding!();

use boltffi::{data, export};

#[export]
pub const U64_BYTES: usize = std::mem::size_of::<u64>();

#[export]
pub async fn answer() -> u32 {
    42
}

#[export]
pub fn invoke(callback: Box<dyn Fn(i32) -> i32 + Send + Sync>, value: i32) -> i32 {
    callback(value)
}

#[data]
#[derive(Clone, Copy)]
#[repr(u64)]
pub enum BigInt {
    Maximum = 0xffffffffffffffff,
}

#[export]
pub fn global_this(global_this: BigInt) -> BigInt {
    global_this
}
