use boltffi::{custom_type, data};
use uom::si::f64;
use uom::si::length::{centimeter, meter};

custom_type! {
    pub LengthMeters,
    remote = uom::si::f64::Length,
    repr = f64,
    into_ffi = |length: &uom::si::f64::Length| length.get::<meter>(),
    try_from_ffi = |meters: f64| Ok(uom::si::f64::Length::new::<meter>(meters)),
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Length {
    value: f64::Length,
}

#[data(impl)]
impl Length {
    #[demo_bench_macros::demo_case(
        "custom_types.length.should_construct_in_meters",
        justification = "A record constructor must encode its private uom field through the custom conversion",
        directions = "Construct Length with 2.5 meters and assert its numeric representation is 2.5"
    )]
    pub fn new(meters: f64) -> Self {
        Self {
            value: f64::Length::new::<meter>(meters),
        }
    }

    #[demo_bench_macros::demo_case(
        "custom_types.length.should_convert_to_centimeters",
        justification = "A record method must reconstruct the private uom field before calling Rust",
        directions = "Call centimeters on a 2.5 meter Length and assert 250"
    )]
    pub fn centimeters(&self) -> f64 {
        self.value.get::<centimeter>()
    }

    #[demo_bench_macros::demo_case(
        "custom_types.length.should_write_back_in_meters",
        justification = "A mutable record method must convert its updated uom field back to meters",
        directions = "Set a Length to 75 centimeters and assert its numeric representation is 0.75 and centimeters returns 75"
    )]
    pub fn set_centimeters(&mut self, centimeters: f64) {
        self.value = f64::Length::new::<centimeter>(centimeters);
    }
}
