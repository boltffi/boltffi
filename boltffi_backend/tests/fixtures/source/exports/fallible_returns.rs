#[repr(C)]
#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[error]
pub enum MathError {
    InvalidInput,
}

#[export]
pub fn checked_add(left: i32, right: i32) -> Result<i32, MathError> {
    Ok(left + right)
}

#[export]
pub fn parse_point(text: String) -> Result<Point, MathError> {
    if text.is_empty() {
        Err(MathError::InvalidInput)
    } else {
        Ok(Point { x: 1, y: 2 })
    }
}

#[export]
pub fn validate_name(name: String) -> Result<String, MathError> {
    Ok(name)
}

#[export]
pub fn store_name(name: String) -> Result<(), MathError> {
    if name.is_empty() {
        Err(MathError::InvalidInput)
    } else {
        Ok(())
    }
}
