boltffi::scaffolding!();

pub mod planar {
    #[boltffi::data]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }
}

pub mod spatial {
    #[boltffi::data]
    #[derive(Clone, Copy)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
        pub z: f64,
    }
}

fn main() {}
