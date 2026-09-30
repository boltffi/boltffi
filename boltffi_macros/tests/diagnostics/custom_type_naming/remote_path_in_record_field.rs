boltffi::scaffolding!();

pub mod units {
    #[derive(Clone, Copy)]
    pub struct Length(pub f64);
}

boltffi::custom_type!(
    pub LengthMeters,
    remote = units::Length,
    repr = f64,
    into_ffi = |length: &units::Length| length.0,
    try_from_ffi = |meters: f64| Ok(units::Length(meters)),
);

#[boltffi::data]
#[derive(Clone, Copy)]
pub struct Plank {
    pub length: units::Length,
}

fn main() {}
