pub struct Engine;

#[export(single_threaded)]
impl Engine {
    pub fn new() -> Self {
        Self
    }

    pub async fn merge(&self, other: &Engine) -> u32 {
        0
    }
}

#[export]
pub async fn inspect(engine: &Engine) -> u32 {
    0
}
