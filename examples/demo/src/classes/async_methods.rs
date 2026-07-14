use boltffi::*;

pub struct AsyncWorker {
    prefix: String,
}

#[export]
impl AsyncWorker {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }

    pub fn get_prefix(&self) -> String {
        self.prefix.clone()
    }

    pub async fn process(&self, input: String) -> String {
        format!("{}: {}", self.prefix, input)
    }

    pub async fn try_process(&self, input: String) -> Result<String, String> {
        if input.is_empty() {
            Err("input must not be empty".to_string())
        } else {
            Ok(format!("{}: {}", self.prefix, input))
        }
    }

    pub async fn find_item(&self, id: i32) -> Option<String> {
        if id > 0 {
            Some(format!("{}_{}", self.prefix, id))
        } else {
            None
        }
    }

    pub async fn process_batch(&self, inputs: Vec<String>) -> Vec<String> {
        inputs
            .into_iter()
            .map(|input| format!("{}: {}", self.prefix, input))
            .collect()
    }
}
