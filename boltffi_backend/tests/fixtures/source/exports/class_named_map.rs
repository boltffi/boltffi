use std::collections::HashMap;

pub struct Map {
    scores: HashMap<String, i32>,
}

#[export]
impl Map {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
        }
    }

    pub fn scores(&self) -> HashMap<String, i32> {
        self.scores.clone()
    }
}
