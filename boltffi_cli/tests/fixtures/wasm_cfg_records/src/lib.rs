boltffi::scaffolding!();

#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Reading {
    pub shared: u32,
    #[cfg(target_arch = "wasm32")]
    pub wasm_only: u8,
    #[cfg(not(target_arch = "wasm32"))]
    pub native_only: u64,
}

#[boltffi::export]
pub fn read(reading: Reading) -> u32 {
    reading.shared
}
