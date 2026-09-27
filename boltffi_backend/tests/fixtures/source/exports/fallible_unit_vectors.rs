#[error]
pub enum ValidationError {
    Empty,
}

#[repr(C)]
#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[export]
pub fn validate_ids(ids: Vec<i64>) -> Result<(), ValidationError> {
    if ids.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(())
    }
}

#[export]
pub fn validate_pairs(ids: Vec<i64>, weights: Vec<f64>) -> Result<(), ValidationError> {
    if ids.is_empty() || weights.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(())
    }
}

#[export]
pub fn validate_points(points: Vec<Point>) -> Result<(), ValidationError> {
    if points.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(())
    }
}

#[export]
pub fn validate_names(ids: Vec<i64>, names: Vec<String>) -> Result<(), ValidationError> {
    if ids.is_empty() || names.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(())
    }
}

#[export]
pub fn validate_slice(ids: &[i64]) -> Result<(), ValidationError> {
    if ids.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(())
    }
}

#[export]
pub fn validate_mutable_slice(ids: &mut [i64]) -> Result<(), ValidationError> {
    if ids.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(())
    }
}

#[export]
pub fn count_ids(ids: Vec<i64>) -> Result<u64, ValidationError> {
    if ids.is_empty() {
        Err(ValidationError::Empty)
    } else {
        Ok(ids.len() as u64)
    }
}

#[export]
pub fn discard_ids(ids: Vec<i64>) {
    drop(ids);
}

pub struct Validator;

#[export]
impl Validator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, ids: Vec<i64>) -> Result<(), ValidationError> {
        validate_ids(ids)
    }
}
