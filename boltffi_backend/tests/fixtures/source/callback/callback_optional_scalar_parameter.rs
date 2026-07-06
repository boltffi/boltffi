#[export]
pub trait Listener {
    fn update(&self, value: Option<u32>);
}

#[export]
pub fn install(listener: Box<dyn Listener>) {}

#[export]
pub fn make_listener() -> Box<dyn Listener> {
    loop {}
}
