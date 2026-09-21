use crate::results::ComputeError;
use crate::{
    enums::{data_enum::Shape, repr_int::Priority},
    records::{
        blittable::Point,
        mixed::{MixedRecord, MixedRecordParameters, echo_mixed_record, make_mixed_record},
    },
};
use boltffi::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

const WAKER_THREAD_NAME: &str = "boltffi-demo-waker";

struct WakeFromNativeThread {
    woken: Arc<AtomicBool>,
    spawned: bool,
}

impl Future for WakeFromNativeThread {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.woken.load(Ordering::Acquire) {
            return Poll::Ready(());
        }
        if !self.spawned {
            self.spawned = true;
            let woken = Arc::clone(&self.woken);
            let waker = cx.waker().clone();
            std::thread::Builder::new()
                .name(WAKER_THREAD_NAME.to_owned())
                .spawn(move || {
                    // Lets the host park its continuation so the wake drives the resume path.
                    std::thread::sleep(Duration::from_millis(20));
                    woken.store(true, Ordering::Release);
                    waker.wake();
                })
                .expect("waker thread should spawn");
        }
        Poll::Pending
    }
}

/// Adds two numbers asynchronously.
#[demo_bench_macros::demo_case(
    "async_fns.basic.add.should_return_sum",
    justification = "Ensure an async i32 addition function resolves with the sum.",
    directions = "Call `async_fns::async_add` through the generated binding and assert an async i32 addition function resolves with the sum."
)]
#[export]
#[demo_bench_macros::benchmark_candidate(function, uniffi, wasm_bindgen)]
pub async fn async_add(a: i32, b: i32) -> i32 {
    a + b
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.echo.should_prefix_message",
    justification = "Ensure an async string function resolves with the expected prefixed message.",
    directions = "Call `async_fns::async_echo` through the generated binding and assert an async string function resolves with the expected prefixed message."
)]
#[export]
pub async fn async_echo(message: String) -> String {
    format!("Echo: {}", message)
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.double_all.should_double_i32_vector",
    justification = "Ensure an async vector function resolves with every i32 value doubled.",
    directions = "Call `async_fns::async_double_all` through the generated binding and assert an async vector function resolves with every i32 value doubled."
)]
#[export]
pub async fn async_double_all(values: Vec<i32>) -> Vec<i32> {
    values.into_iter().map(|v| v * 2).collect()
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.find_positive.should_return_first_positive",
    justification = "Ensure an async optional result resolves with the first positive i32 in a vector.",
    directions = "Call `async_fns::async_find_positive` through the generated binding and assert an async optional result resolves with the first positive i32 in a vector."
)]
#[demo_bench_macros::demo_case(
    "async_fns.basic.find_positive.should_return_none_for_all_negative",
    justification = "Ensure an async optional result resolves to none when no positive i32 is present.",
    directions = "Call `async_fns::async_find_positive` through the generated binding and assert an async optional result resolves to none when no positive i32 is present."
)]
#[export]
pub async fn async_find_positive(values: Vec<i32>) -> Option<i32> {
    values.into_iter().find(|&v| v > 0)
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.concat.should_join_string_vector",
    justification = "Ensure an async string-vector function resolves with the values joined by commas.",
    directions = "Call `async_fns::async_concat` through the generated binding and assert an async string-vector function resolves with the values joined by commas."
)]
#[export]
pub async fn async_concat(strings: Vec<String>) -> String {
    strings.join(", ")
}

#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_doubled_value",
    justification = "Ensure an async Result function resolves with a doubled value for valid input.",
    directions = "Call `async_fns::try_compute_async` through the generated binding and assert an async Result function resolves with a doubled value for valid input."
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_overflow_for_negative_value",
    justification = "Ensure an async Result function rejects negative input with the typed overflow error.",
    directions = "Call `async_fns::try_compute_async` through the generated binding and assert an async Result function rejects negative input with the typed overflow error."
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_invalid_input_for_zero",
    justification = "Ensure an async Result function rejects zero input with the typed invalid-input error.",
    directions = "Call `async_fns::try_compute_async` through the generated binding and assert an async Result function rejects zero input with the typed invalid-input error."
)]
#[export]
pub async fn try_compute_async(value: i32) -> Result<i32, ComputeError> {
    crate::results::try_compute(value)
}

#[demo_bench_macros::demo_case(
    "async_fns.results.fetch_data.should_return_scaled_positive_id",
    justification = "Ensure an async string-error Result function resolves with a scaled positive id.",
    directions = "Call `async_fns::fetch_data` through the generated binding and assert an async string-error Result function resolves with a scaled positive id."
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.fetch_data.should_reject_non_positive_id",
    justification = "Ensure an async string-error Result function rejects a non-positive id.",
    directions = "Call `async_fns::fetch_data` through the generated binding and assert an async string-error Result function rejects a non-positive id."
)]
#[export]
pub async fn fetch_data(id: i32) -> Result<i32, String> {
    if id > 0 {
        Ok(id * 10)
    } else {
        Err("invalid id".to_string())
    }
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.get_numbers.should_return_counting_sequence",
    justification = "Ensure an async vector producer resolves with a zero-based counting sequence.",
    directions = "Call `async_fns::async_get_numbers` through the generated binding and assert an async vector producer resolves with a zero-based counting sequence."
)]
#[export]
#[demo_bench_macros::benchmark_candidate(function, uniffi)]
pub async fn async_get_numbers(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[demo_bench_macros::demo_case(
    "async_fns.mixed_record.echo.should_roundtrip_record",
    justification = "Ensure an async function round-trips a mixed record containing nested records and enums.",
    directions = "Call `async_fns::async_echo_mixed_record` through the generated binding and assert an async function round-trips a mixed record containing nested records and enums."
)]
#[export]
pub async fn async_echo_mixed_record(record: MixedRecord) -> MixedRecord {
    echo_mixed_record(record)
}

#[demo_bench_macros::demo_case(
    "async_fns.mixed_record.make.should_construct_record",
    justification = "Ensure an async function constructs a mixed record from scalar, record, enum, and nested parameters.",
    directions = "Call `async_fns::async_make_mixed_record` through the generated binding and assert an async function constructs a mixed record from scalar, record, enum, and nested parameters."
)]
#[export]
pub async fn async_make_mixed_record(
    name: String,
    anchor: Point,
    priority: Priority,
    shape: Shape,
    parameters: MixedRecordParameters,
) -> MixedRecord {
    make_mixed_record(name, anchor, priority, shape, parameters)
}

#[demo_bench_macros::demo_case(
    "async_fns.native_wake.resumed_thread.should_not_be_the_waking_thread",
    justification = "Ensure hosts resume an async call on their own threads instead of inline on the Rust thread that woke the future, which may be a runtime worker that must not block, run host callbacks, or drop native objects.",
    directions = "Await `async_fns::async_resumed_thread_name` through the generated binding and assert the returned thread name is not `boltffi-demo-waker`.",
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "wasm32 has no native threads, so nothing can wake the future from another thread."
    )
)]
#[export]
pub async fn async_resumed_thread_name() -> String {
    WakeFromNativeThread {
        woken: Arc::new(AtomicBool::new(false)),
        spawned: false,
    }
    .await;
    std::thread::current().name().unwrap_or_default().to_owned()
}
