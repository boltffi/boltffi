use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use boltffi::export;

static DROPS: AtomicU32 = AtomicU32::new(0);

pub struct Message {
    text: String,
}

impl Drop for Message {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

#[export]
impl Message {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn length(&self) -> u32 {
        self.text.len() as u32
    }

    pub fn combine(&self, other: Message) -> u32 {
        (self.text.len() + other.text.len()) as u32
    }
}

pub struct Sink {
    count: AtomicU32,
}

#[export]
impl Sink {
    pub fn new() -> Self {
        Self {
            count: AtomicU32::new(0),
        }
    }

    pub fn from_message(message: Message) -> Self {
        Self {
            count: AtomicU32::new(message.text.len() as u32),
        }
    }

    pub async fn set(
        &self,
        message: Message,
        carrier: HashMap<String, String>,
    ) -> Result<(), String> {
        self.count.store(
            (message.text.len() + carrier.len()) as u32,
            Ordering::SeqCst,
        );
        Ok(())
    }
}

#[export]
pub fn consume(message: Message) -> u32 {
    message.text.len() as u32
}

#[export]
pub fn discard(message: Message) {
    drop(message);
}

#[export]
pub fn consume_optional(message: Option<Message>) -> u32 {
    message
        .as_ref()
        .map_or(0, |message| message.text.len() as u32)
}

#[export]
pub fn consume_pair(first: Message, second: Message) -> u32 {
    (first.text.len() + second.text.len()) as u32
}

#[export]
pub fn consume_fallible(message: Message) -> Result<u32, String> {
    if message.text.is_empty() {
        Err("empty message".to_owned())
    } else {
        Ok(message.text.len() as u32)
    }
}

#[export]
pub fn consume_encoded(labels: Vec<String>, message: Message) -> String {
    labels.join(&message.text)
}

#[export]
pub fn echo(message: Message) -> Message {
    message
}

#[export]
pub fn inspect(message: &Message) -> u32 {
    message.text.len() as u32
}

#[export]
pub fn modify(message: &mut Message) {
    message.text.push('!');
}

#[export]
pub async fn hold(message: &Message) -> u32 {
    std::future::pending::<()>().await;
    message.text.len() as u32
}

#[export]
pub async fn hold_owned(message: Message) -> u32 {
    std::future::pending::<()>().await;
    message.text.len() as u32
}

#[export]
pub async fn consume_async(message: Message) -> Result<u32, String> {
    consume_fallible(message)
}

#[export]
pub async fn consume_pair_async(first: Message, second: Message) -> u32 {
    consume_pair(first, second)
}

#[export]
pub async fn consume_optional_async(message: Option<Message>) -> u32 {
    consume_optional(message)
}

#[export]
pub fn missing_entry(message: Message) -> u32 {
    consume(message)
}

#[export]
pub fn dropped_messages() -> u32 {
    DROPS.load(Ordering::SeqCst)
}
