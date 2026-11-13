boltffi::scaffolding!();

#[boltffi::data]
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct Sample {
    pub flag: u8,
    pub value: u32,
}

fn main() {}
