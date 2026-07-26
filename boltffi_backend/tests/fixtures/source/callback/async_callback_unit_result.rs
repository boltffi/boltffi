#[repr(i32)]
#[data]
pub enum LoadError {
    Bad = 1,
}

#[export]
pub trait Listener {
    async fn remove(&self, key: u32) -> Result<(), LoadError>;
}
