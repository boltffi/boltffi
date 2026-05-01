use boltffi::*;

#[export]
pub fn result_of_option(key: i32) -> Result<Option<i32>, String> {
    if key < 0 {
        Err("invalid key".to_string())
    } else if key == 0 {
        Ok(None)
    } else {
        Ok(Some(key * 2))
    }
}

#[export]
pub fn result_of_vec(count: i32) -> Result<Vec<i32>, String> {
    if count < 0 {
        Err("negative count".to_string())
    } else {
        Ok((0..count).collect())
    }
}

#[export]
pub fn result_of_string(key: i32) -> Result<String, String> {
    if key < 0 {
        Err("invalid key".to_string())
    } else {
        Ok(format!("item_{}", key))
    }
}
