#[export]
pub trait Producer {
    fn values(&self) -> Vec<i32>;
}

#[export]
pub fn make_producer() -> Box<dyn Producer> {
    loop {}
}
