pub struct Engine;

#[export]
impl Engine {
    pub fn new() -> Self {
        Self
    }

    pub async fn compute(&self, value: u32) -> u32 {
        value
    }
}
