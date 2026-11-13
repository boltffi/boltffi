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

fn main() {}
