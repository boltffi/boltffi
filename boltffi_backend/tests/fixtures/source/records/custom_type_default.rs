#[data]
pub struct LengthFFI {
    pub meters: f64,
}

custom_type!(
    pub Length,
    remote = LengthRust,
    repr = LengthFFI,
    into_ffi = length_into_ffi,
    try_from_ffi = length_from_ffi
);

#[data]
pub struct DeviationConfig {
    #[boltffi::default(1_500.0)]
    pub max_rejoin_distance: LengthRust,
}
