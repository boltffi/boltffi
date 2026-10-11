pim_macros::scaffolding!();

pub struct NotData {
    pub x: u64,
}

#[pim_macros::export]
pub fn takes(argument: NotData) -> u64 {
    argument.x
}

fn main() {}
