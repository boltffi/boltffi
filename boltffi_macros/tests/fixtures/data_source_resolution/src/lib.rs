boltffi::scaffolding!();

pub mod coordinate;
pub mod cycles;
pub mod event;
pub mod methods;
#[cfg(not(feature = "experimental"))]
#[path = "mode_default.rs"]
pub mod mode;
#[cfg(feature = "experimental")]
#[path = "mode_experimental.rs"]
pub mod mode;
pub mod route;

pub use coordinate::GeographicCoordinate;
pub use event::RoadEvent;
pub use mode::TravelMode;
