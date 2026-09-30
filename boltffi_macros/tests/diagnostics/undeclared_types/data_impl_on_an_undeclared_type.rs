boltffi::scaffolding!();

#[derive(Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[boltffi::data(impl)]
impl Point {
    pub fn norm(&self) -> f64 {
        self.x.hypot(self.y)
    }
}

fn main() {}
