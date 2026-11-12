use std::time::{Duration, SystemTime};

use uuid::Uuid;

#[export]
pub fn echo_text(value: String) -> String {
    value
}

#[export]
pub fn echo_ratio(value: f32) -> f32 {
    value
}

#[export]
pub fn echo_ratios(value: Vec<f32>) -> Vec<f32> {
    value
}

#[export]
pub fn echo_bytes(value: Vec<u8>) -> Vec<u8> {
    value
}

#[export]
pub fn echo_id(value: Uuid) -> Uuid {
    value
}

#[export]
pub fn echo_time(value: SystemTime) -> SystemTime {
    value
}

#[export]
pub fn echo_span(value: Duration) -> Duration {
    value
}
