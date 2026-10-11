use pim_macros::{custom_type, data};

custom_type!(std::time::Duration);

#[data]
pub struct Deadline {
    pub after: std::time::Duration,
    pub label: String,
}
