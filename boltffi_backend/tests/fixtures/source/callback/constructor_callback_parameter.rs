use std::sync::Arc;

#[export]
pub trait Notifier {
    fn notify(&self, code: u32);
}

pub struct Engine;

#[export]
impl Engine {
    pub fn new(notifier: Arc<dyn Notifier>) -> Self {
        Self
    }

    pub fn ping(&self) -> u32 {
        1
    }
}
