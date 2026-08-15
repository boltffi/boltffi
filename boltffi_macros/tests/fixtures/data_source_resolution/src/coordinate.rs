#[boltffi::data]
#[derive(Clone, Copy)]
pub struct GeographicCoordinate {
    pub latitude: f64,
    pub longitude: f64,
}
