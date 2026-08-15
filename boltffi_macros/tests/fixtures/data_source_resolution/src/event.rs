use crate::GeographicCoordinate;

#[boltffi::data]
pub enum RoadEvent {
    Detected { location: GeographicCoordinate },
}
