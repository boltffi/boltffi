boltffi::scaffolding!();

pub mod units {
    pub struct Length(pub f64);
}

use units::Length;

boltffi::custom_type!(
    pub LengthMeters,
    remote = units::Length,
    repr = f64,
    into_ffi = |length: &units::Length| length.0,
    try_from_ffi = |meters: f64| Ok(units::Length(meters)),
);

#[boltffi::export]
pub fn double(length: Length) -> Length {
    Length(length.0 * 2.0)
}

fn main() {}
