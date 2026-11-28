#[export]
pub struct Engine {
    value: i32,
}

#[export]
impl Engine {
    pub fn new(value: i32) -> Self {
        Self { value }
    }

    pub fn version() -> u32 {
        1
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn swap(&self, other: Engine) -> Engine {
        other
    }

    pub fn maybe_swap(&self, other: Option<Engine>) -> Option<Engine> {
        other
    }
}
