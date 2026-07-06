#[repr(C)]
#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(i32)]
#[data]
pub enum LoadError {
    Bad = 1,
}

#[export]
pub trait Listener {
    async fn value(&self, key: u32) -> u32;
    async fn point(&self, point: Point) -> Point;
    async fn values(&self, count: u32) -> Vec<u32>;
    async fn try_load(&self, key: u32) -> Result<String, LoadError>;
    async fn check_enabled(&self, key: u32) -> Result<bool, LoadError>;
}
