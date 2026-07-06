use boltffi::EventSubscription;
use std::sync::Arc;

#[repr(C)]
#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub struct Engine;

#[export(single_threaded)]
impl Engine {
    #[ffi_stream(item = Point, mode = "batch")]
    pub fn points(&self) -> Arc<EventSubscription<Point>> {
        loop {}
    }

    #[ffi_stream(item = String)]
    pub fn names(&self) -> Arc<EventSubscription<String>> {
        loop {}
    }

    #[ffi_stream(item = i32, mode = "callback")]
    pub fn ticks(&self) -> Arc<EventSubscription<i32>> {
        loop {}
    }
}
