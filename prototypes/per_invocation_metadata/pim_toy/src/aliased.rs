use crate::geometry::Point as GeoPoint;
use pim_macros::data;

#[data]
pub struct Route {
    pub start: GeoPoint,
    pub end: GeoPoint,
}
