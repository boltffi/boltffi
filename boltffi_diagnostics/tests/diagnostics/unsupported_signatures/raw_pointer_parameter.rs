boltffi::scaffolding!();

#[boltffi::export]
pub fn first(bytes: *const u8) -> u8 {
    unsafe { *bytes }
}

fn main() {}
