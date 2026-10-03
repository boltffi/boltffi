pim_macros::scaffolding!();

pub struct NotData {
    pub x: u64,
}

#[pim_macros::data]
pub struct Holder {
    pub inner: NotData,
}

fn main() {}
