#[repr(C)]
#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[export(single_threaded)]
pub struct Engine;

#[export]
impl Engine {
    pub fn new(seed: u64) -> Self {
        Self
    }

    pub fn version() -> u32 {
        1
    }

    pub fn score(&self, point: Point) -> u32 {
        point.x as u32
    }

    pub fn advance(&mut self, delta: u32) {}
}
