use boltffi::*;

scaffolding!();
use std::sync::atomic::{AtomicI32, Ordering};

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ForeignKind {
    #[default]
    Standard,
    Express,
    Archive,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ForeignPoint {
    pub x: f64,
    pub y: f64,
}

#[data]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ForeignUser {
    pub name: String,
    pub age: u32,
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum ForeignState {
    Ready,
    Busy { reason: String },
}

pub struct ForeignCode(String);

impl ForeignCode {
    pub fn new(value: String) -> Result<Self, String> {
        if value.is_empty() {
            Err("foreign code cannot be empty".to_string())
        } else {
            Ok(Self(value))
        }
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

#[custom_ffi]
impl CustomFfiConvertible for ForeignCode {
    type FfiRepr = String;
    type Error = String;

    fn into_ffi(&self) -> String {
        self.0.clone()
    }

    fn try_from_ffi(repr: String) -> Result<Self, String> {
        Self::new(repr)
    }
}

pub struct ForeignCounter {
    value: AtomicI32,
}

#[export]
impl ForeignCounter {
    pub fn new(initial: i32) -> Self {
        Self {
            value: AtomicI32::new(initial),
        }
    }

    pub fn add(&self, amount: i32) -> i32 {
        self.value.fetch_add(amount, Ordering::SeqCst) + amount
    }

    pub fn get(&self) -> i32 {
        self.value.load(Ordering::SeqCst)
    }

    pub fn summarize_user(&self, user: &ForeignUser, kind: ForeignKind) -> String {
        format!(
            "{}#{}#{}#{}",
            user.name,
            user.age,
            kind_label(kind),
            self.get()
        )
    }
}

#[export]
pub trait ForeignLabeler {
    fn label(&self, user: ForeignUser, kind: ForeignKind) -> String;
}

#[export]
pub fn model_echo_kind(kind: ForeignKind) -> ForeignKind {
    kind
}

#[export]
pub fn model_kind_label(kind: ForeignKind) -> String {
    kind_label(kind)
}

#[export]
pub fn model_shift_point(point: ForeignPoint, dx: f64, dy: f64) -> ForeignPoint {
    ForeignPoint {
        x: point.x + dx,
        y: point.y + dy,
    }
}

#[export]
pub fn model_point_sum(point: &ForeignPoint) -> f64 {
    point.x + point.y
}

#[export]
pub fn model_user_summary(user: ForeignUser) -> String {
    format!("{}#{}", user.name, user.age)
}

#[export]
pub fn model_echo_code(code: ForeignCode) -> ForeignCode {
    code
}

#[export]
pub fn model_code_value(code: ForeignCode) -> String {
    code.value().to_string()
}

#[export]
pub fn model_state_summary(state: ForeignState) -> String {
    match state {
        ForeignState::Ready => "ready".to_string(),
        ForeignState::Busy { reason } => format!("busy:{reason}"),
    }
}

#[export]
pub fn model_format_with_labeler(
    labeler: impl ForeignLabeler,
    user: ForeignUser,
    kind: ForeignKind,
) -> String {
    labeler.label(user, kind)
}

pub fn kind_label(kind: ForeignKind) -> String {
    match kind {
        ForeignKind::Standard => "standard",
        ForeignKind::Express => "express",
        ForeignKind::Archive => "archive",
    }
    .to_string()
}
