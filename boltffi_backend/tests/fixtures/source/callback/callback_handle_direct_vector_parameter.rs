#[export]
pub trait Listener {
    fn process(&self, values: Vec<i32>) -> u32;
}

#[export]
pub fn make_listener() -> Box<dyn Listener> {
    loop {}
}
