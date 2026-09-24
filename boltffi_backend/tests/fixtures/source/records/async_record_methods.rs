#[data]
pub struct Marker {}

#[data(impl)]
impl Marker {
    pub async fn fetch(&self) -> u32 {
        7
    }
}

#[data]
pub struct Label {
    pub text: String,
}

#[data(impl)]
impl Label {
    pub async fn measure(&self) -> u32 {
        self.text.len() as u32
    }
}

#[repr(C)]
#[data]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[data(impl)]
impl Point {
    pub async fn length(&self) -> f64 {
        self.x
    }
}
