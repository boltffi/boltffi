#[repr(C)]
#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[export]
pub async fn refresh() {}

#[export]
pub async fn load_name() -> String {
    "bolt".to_owned()
}

#[export]
pub async fn load_point() -> Point {
    Point { x: 1, y: 2 }
}

#[error]
pub enum SaveError {
    Failed,
}

#[export]
pub async fn save() -> Result<(), SaveError> {
    Ok(())
}

#[export]
pub async fn save_checked(value: i32) -> Result<i32, SaveError> {
    Ok(value)
}
