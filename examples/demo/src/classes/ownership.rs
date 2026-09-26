use std::collections::HashMap;
use std::future::pending;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use boltffi::export;

#[derive(Default)]
pub struct MessageDrops {
    count: Arc<AtomicU32>,
}

#[export]
impl MessageDrops {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }
}

pub struct OwnedMessage {
    text: String,
    drops: Arc<AtomicU32>,
}

#[export]
impl OwnedMessage {
    pub fn new(text: String, drops: &MessageDrops) -> Self {
        Self {
            text,
            drops: Arc::clone(&drops.count),
        }
    }

    pub fn length(&self) -> u32 {
        self.text.len() as u32
    }

    pub fn combine(&self, other: OwnedMessage) -> u32 {
        self.length() + other.length()
    }
}

impl Drop for OwnedMessage {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub struct MessageStore {
    count: AtomicU32,
}

#[export]
impl MessageStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_message(message: OwnedMessage) -> Self {
        Self {
            count: AtomicU32::new(message.length()),
        }
    }

    pub fn count(&self) -> u32 {
        self.count.load(Ordering::SeqCst)
    }

    pub async fn set(
        &self,
        message: OwnedMessage,
        carrier: HashMap<String, String>,
    ) -> Result<(), String> {
        self.count
            .store(message.length() + carrier.len() as u32, Ordering::SeqCst);
        Ok(())
    }
}

#[export]
pub fn consume_message(message: OwnedMessage) -> u32 {
    message.length()
}

#[export]
pub fn discard_message(message: OwnedMessage) {
    drop(message);
}

#[export]
pub fn consume_optional_message(message: Option<OwnedMessage>) -> u32 {
    message.as_ref().map_or(0, OwnedMessage::length)
}

#[export]
pub fn consume_messages(first: OwnedMessage, second: OwnedMessage) -> u32 {
    first.length() + second.length()
}

#[export]
pub fn consume_message_result(message: OwnedMessage) -> Result<u32, String> {
    if message.text.is_empty() {
        Err("empty message".to_owned())
    } else {
        Ok(message.length())
    }
}

#[export]
pub fn join_message(labels: Vec<String>, message: OwnedMessage) -> String {
    labels.join(&message.text)
}

#[export]
pub fn return_message(message: OwnedMessage) -> OwnedMessage {
    message
}

#[export]
pub fn borrow_message(message: &OwnedMessage) -> u32 {
    message.length()
}

#[export]
pub fn mutate_message(message: &mut OwnedMessage) {
    message.text.push('!');
}

#[export]
pub async fn hold_message(message: &OwnedMessage) -> u32 {
    pending::<()>().await;
    message.length()
}

#[export]
pub async fn hold_owned_message(message: OwnedMessage) -> u32 {
    pending::<()>().await;
    message.length()
}

#[export]
pub async fn consume_message_async(message: OwnedMessage) -> Result<u32, String> {
    consume_message_result(message)
}

#[export]
pub async fn consume_messages_async(first: OwnedMessage, second: OwnedMessage) -> u32 {
    consume_messages(first, second)
}

#[export]
pub async fn consume_optional_message_async(message: Option<OwnedMessage>) -> u32 {
    consume_optional_message(message)
}

#[export]
pub fn consume_message_with_offset(
    message: OwnedMessage,
    message_owned_handle: u32,
    boltffi_call_result: u32,
    __boltffi_owned_handle0: u32,
) -> u32 {
    message.length() + message_owned_handle + boltffi_call_result + __boltffi_owned_handle0
}
