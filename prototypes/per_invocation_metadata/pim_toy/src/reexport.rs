use pim_macros::data;

pub use crate::geometry::Shape;

#[data]
pub struct Drawing {
    pub shape: crate::reexport::Shape,
}
