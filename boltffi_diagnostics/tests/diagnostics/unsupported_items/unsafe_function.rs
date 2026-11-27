boltffi::scaffolding!();

#[boltffi::export]
pub unsafe fn read(address: usize) -> u8 {
    unsafe { *(address as *const u8) }
}

fn main() {}
