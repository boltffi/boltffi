boltffi::scaffolding!();

pub mod units {
    pub struct Length(pub f64);
}

boltffi::custom_type!(
    pub LengthMeters,
    remote = units::Length,
    repr = f64,
    into_ffi = |length: &units::Length| length.0,
    try_from_ffi = |meters: f64| Ok(units::Length(meters)),
);

#[boltffi::export]
pub fn double(length: units::Length) -> units::Length {
    units::Length(length.0 * 2.0)
}

fn main() {}
