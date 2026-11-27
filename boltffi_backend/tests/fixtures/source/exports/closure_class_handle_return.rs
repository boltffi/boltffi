#[export(single_threaded)]
pub struct Engine;

#[export]
impl Engine {
    pub fn new() -> Self {
        Self
    }
}

#[export]
pub fn install(callback: impl Fn() -> Engine) {}
