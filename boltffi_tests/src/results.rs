use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use boltffi::*;

use crate::TestCounter;
use crate::{FixtureMessageRecord, FixtureRect, FixtureShape, FixtureStatus};

struct YieldOnce(bool);

impl YieldOnce {
    fn new() -> Self {
        Self(false)
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[data]
#[derive(Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum FixtureError {
    NotFound = 1,
    InvalidInput = 2,
    Timeout = 3,
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found"),
            Self::InvalidInput => write!(f, "invalid input"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

impl std::error::Error for FixtureError {}

#[export]
pub fn fallible_divide(a: i32, b: i32) -> Result<i32, FixtureError> {
    if b == 0 {
        Err(FixtureError::InvalidInput)
    } else {
        Ok(a / b)
    }
}

#[export]
pub fn fallible_lookup(key: i32) -> Result<String, FixtureError> {
    match key {
        1 => Ok("one".to_string()),
        2 => Ok("two".to_string()),
        3 => Ok("three".to_string()),
        _ => Err(FixtureError::NotFound),
    }
}

#[export]
pub async fn async_fallible_fetch(key: i32) -> Result<String, FixtureError> {
    if key < 0 {
        Err(FixtureError::InvalidInput)
    } else if key > 100 {
        Err(FixtureError::NotFound)
    } else {
        Ok(format!("value_{}", key))
    }
}

pub struct CancellableTask {
    started: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    iterations: Arc<AtomicU32>,
}

impl Default for CancellableTask {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl CancellableTask {
    pub fn new() -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
            completed: Arc::new(AtomicBool::new(false)),
            iterations: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn was_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    pub fn was_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    pub fn iteration_count(&self) -> u32 {
        self.iterations.load(Ordering::SeqCst)
    }

    pub async fn long_running_task(&self) -> i32 {
        self.started.store(true, Ordering::SeqCst);

        let mut iteration = 0;
        while iteration < 100 {
            self.iterations.store(iteration, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            YieldOnce::new().await;
            iteration += 1;
        }

        self.completed.store(true, Ordering::SeqCst);
        42
    }

    pub async fn instant_task(&self) -> i32 {
        self.started.store(true, Ordering::SeqCst);
        self.completed.store(true, Ordering::SeqCst);
        99
    }
}

pub struct FallibleService {
    failure_mode: Arc<AtomicU32>,
}

impl Default for FallibleService {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl FallibleService {
    pub fn new() -> Self {
        Self {
            failure_mode: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn set_failure_mode(&self, mode: u32) {
        self.failure_mode.store(mode, Ordering::SeqCst);
    }

    pub fn get_value(&self, key: i32) -> Result<i32, FixtureError> {
        match self.failure_mode.load(Ordering::SeqCst) {
            1 => Err(FixtureError::NotFound),
            2 => Err(FixtureError::InvalidInput),
            3 => Err(FixtureError::Timeout),
            _ => Ok(key * 2),
        }
    }

    pub async fn async_get_value(&self, key: i32) -> Result<i32, FixtureError> {
        self.get_value(key)
    }

    pub fn get_optional(&self, key: i32) -> Option<i32> {
        if key > 0 { Some(key * 3) } else { None }
    }

    pub fn get_nested_result(&self, key: i32) -> Result<Option<i32>, FixtureError> {
        match self.failure_mode.load(Ordering::SeqCst) {
            1 => Err(FixtureError::NotFound),
            _ if key < 0 => Ok(None),
            _ => Ok(Some(key * 4)),
        }
    }

    /// Returns a fresh `TestCounter` class handle, or an error when the
    /// requested initial value is negative. Exercises the
    /// `Result<Class, Error>` return shape where the success payload must
    /// lower as a class handle, not as an encoded record.
    pub fn try_make_counter(&self, initial: i32) -> Result<TestCounter, FixtureError> {
        if initial < 0 {
            Err(FixtureError::InvalidInput)
        } else {
            Ok(TestCounter::new(initial))
        }
    }
}

#[export]
pub fn simple_try_divide(a: i32, b: i32) -> Result<i32, i32> {
    if b != 0 { Ok(a / b) } else { Err(-1) }
}

#[export]
pub fn try_divide(a: i32, b: i32) -> Result<i32, String> {
    if b == 0 {
        Err("divide by zero".to_string())
    } else {
        Ok(a / b)
    }
}

#[export]
pub fn try_ping(fail: bool) -> Result<(), String> {
    if fail {
        Err("ping failed".to_string())
    } else {
        Ok(())
    }
}

#[export]
pub fn try_greet(fail: bool) -> Result<String, String> {
    if fail {
        Err("greet failed".to_string())
    } else {
        Ok("hello".to_string())
    }
}

#[export]
pub fn try_rect(fail: bool) -> Result<FixtureRect, String> {
    if fail {
        Err("rect failed".to_string())
    } else {
        Ok(FixtureRect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        })
    }
}

#[export]
pub fn try_message(fail: bool) -> Result<FixtureMessageRecord, String> {
    if fail {
        Err("message failed".to_string())
    } else {
        Ok(FixtureMessageRecord {
            label: "ok".to_string(),
            anchor: crate::FixturePoint { x: 1.0, y: 1.5 },
            status: FixtureStatus::Completed,
        })
    }
}

#[export]
pub fn try_status_err(x: i32) -> Result<i32, FixtureStatus> {
    if x >= 0 {
        Ok(x + 1)
    } else {
        Err(FixtureStatus::Failed)
    }
}

#[export]
pub fn try_shape_err(x: i32) -> Result<i32, FixtureShape> {
    if x >= 0 {
        Ok(x + 2)
    } else {
        Err(FixtureShape::Line(x.abs() as f64))
    }
}
