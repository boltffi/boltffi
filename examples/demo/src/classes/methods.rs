use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use demo_bench_macros::benchmark_candidate;

use crate::{
    enums::{data_enum::Shape, repr_int::Priority},
    records::{
        blittable::Point,
        mixed::{MixedRecord, MixedRecordParameters, make_mixed_record},
    },
};

#[cfg(not(feature = "uniffi"))]
use boltffi::*;

/// A simple thread-safe counter that demonstrates various
/// method return types: plain values, Option, Result, and
/// records.
#[benchmark_candidate(object, uniffi)]
pub struct Counter {
    count: Mutex<i32>,
}

#[cfg(not(feature = "uniffi"))]
#[export]
#[benchmark_candidate(impl, uniffi)]
impl Counter {
    pub fn new(initial: i32) -> Counter {
        Counter {
            count: Mutex::new(initial),
        }
    }

    pub fn get(&self) -> i32 {
        *self.count.lock().unwrap()
    }

    pub fn increment(&self) {
        *self.count.lock().unwrap() += 1;
    }

    pub fn add(&self, amount: i32) {
        *self.count.lock().unwrap() += amount;
    }

    pub fn reset(&self) {
        *self.count.lock().unwrap() = 0;
    }

    pub fn try_reset_if_positive(&self) -> Result<(), String> {
        let mut count = self.count.lock().unwrap();
        if *count > 0 {
            *count = 0;
            Ok(())
        } else {
            Err("count is not positive".to_string())
        }
    }

    /// Returns the current count if positive, or an error message.
    pub fn try_get_positive(&self) -> Result<i32, String> {
        let val = *self.count.lock().unwrap();
        if val > 0 {
            Ok(val)
        } else {
            Err("count is not positive".to_string())
        }
    }

    pub fn maybe_double(&self) -> Option<i32> {
        let val = *self.count.lock().unwrap();
        if val != 0 { Some(val * 2) } else { None }
    }

    pub fn as_point(&self) -> Point {
        Point {
            x: *self.count.lock().unwrap() as f64,
            y: 0.0,
        }
    }
}

#[cfg(not(feature = "uniffi"))]
#[export(methods)]
impl Counter {
    #[demo_bench_macros::demo_case(
        "classes.methods.counter.decrement.should_call_a_method_declared_in_a_methods_block",
        justification = "Ensure a method written in an `#[export(methods)]` block joins the class that its `#[export] impl` block declares.",
        directions = "Construct `classes::methods::Counter` through the generated binding, call `decrement`, and assert the count dropped by one."
    )]
    pub fn decrement(&self) {
        *self.count.lock().unwrap() -= 1;
    }
}

#[cfg(feature = "uniffi")]
#[benchmark_candidate(impl, uniffi, constructor = "new")]
impl Counter {
    pub fn new(initial: i32) -> Counter {
        Counter {
            count: Mutex::new(initial),
        }
    }

    pub fn get(&self) -> i32 {
        *self.count.lock().unwrap()
    }

    pub fn increment(&self) {
        *self.count.lock().unwrap() += 1;
    }

    pub fn add(&self, amount: i32) {
        *self.count.lock().unwrap() += amount;
    }

    pub fn reset(&self) {
        *self.count.lock().unwrap() = 0;
    }
}

pub struct MixedRecordService {
    label: String,
    pub(crate) stored_count: AtomicU32,
}

#[cfg(not(feature = "uniffi"))]
#[export]
impl MixedRecordService {
    pub fn new(label: String) -> Self {
        Self {
            label,
            stored_count: AtomicU32::new(0),
        }
    }

    pub fn get_label(&self) -> String {
        self.label.clone()
    }

    pub fn stored_count(&self) -> u32 {
        self.stored_count.load(Ordering::Relaxed)
    }

    pub fn echo_record(&self, record: MixedRecord) -> MixedRecord {
        record
    }

    pub fn store_record_parts(
        &self,
        name: String,
        anchor: Point,
        priority: Priority,
        shape: Shape,
        parameters: MixedRecordParameters,
    ) -> MixedRecord {
        self.stored_count.fetch_add(1, Ordering::Relaxed);
        make_mixed_record(name, anchor, priority, shape, parameters)
    }

    pub async fn async_echo_record(&self, record: MixedRecord) -> MixedRecord {
        record
    }

    pub async fn async_store_record_parts(
        &self,
        name: String,
        anchor: Point,
        priority: Priority,
        shape: Shape,
        parameters: MixedRecordParameters,
    ) -> MixedRecord {
        self.stored_count.fetch_add(1, Ordering::Relaxed);
        make_mixed_record(name, anchor, priority, shape, parameters)
    }
}
