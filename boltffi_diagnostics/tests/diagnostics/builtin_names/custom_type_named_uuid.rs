boltffi::scaffolding!();

pub mod ids {
    pub struct Token(pub u64);
}

boltffi::custom_type!(
    pub Uuid,
    remote = ids::Token,
    repr = u64,
    into_ffi = |token: &ids::Token| token.0,
    try_from_ffi = |raw: u64| Ok(ids::Token(raw)),
);

fn main() {}
