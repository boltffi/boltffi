use boltffi_bindgen::generate::Generation;

pub struct TargetGeneration {
    target: String,
    generation: Generation,
}

impl TargetGeneration {
    pub fn new(target: impl Into<String>, generation: Generation) -> Self {
        Self {
            target: target.into(),
            generation,
        }
    }

    pub fn into_parts(self) -> (String, Generation) {
        (self.target, self.generation)
    }
}
