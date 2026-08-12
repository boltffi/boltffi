use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use boltffi::*;

// Self-rewaking rather than a real timer/thread: wasm32 has neither.
struct Yield(u32);

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 == 0 {
            Poll::Ready(())
        } else {
            self.0 -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

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

    pub async fn process_after_polls(&self, input: String, polls: u32) -> String {
        Yield(polls).await;
        format!("{}: {}", self.prefix, input)
    }
}
