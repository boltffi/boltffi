use crate::results::ComputeError;
use crate::{
    enums::{data_enum::Shape, repr_int::Priority},
    records::{
        blittable::Point,
        mixed::{MixedRecord, MixedRecordParameters, echo_mixed_record, make_mixed_record},
    },
};
use boltffi::*;

/// Adds two numbers asynchronously.
#[demo_bench_macros::demo_case(
    "async_fns.basic.add.should_return_sum",
    description = "An async i32 addition function resolves with the sum.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
#[demo_bench_macros::benchmark_candidate(function, uniffi, wasm_bindgen)]
pub async fn async_add(a: i32, b: i32) -> i32 {
    a + b
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.echo.should_prefix_message",
    description = "An async string function resolves with the expected prefixed message.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
pub async fn async_echo(message: String) -> String {
    format!("Echo: {}", message)
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.double_all.should_double_i32_vector",
    description = "An async vector function resolves with every i32 value doubled.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
pub async fn async_double_all(values: Vec<i32>) -> Vec<i32> {
    values.into_iter().map(|v| v * 2).collect()
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.find_positive.should_return_first_positive",
    description = "An async optional result resolves with the first positive i32 in a vector.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.basic.find_positive.should_return_none_for_all_negative",
    description = "An async optional result resolves to none when no positive i32 is present.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
pub async fn async_find_positive(values: Vec<i32>) -> Option<i32> {
    values.into_iter().find(|&v| v > 0)
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.concat.should_join_string_vector",
    description = "An async string-vector function resolves with the values joined by commas.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
pub async fn async_concat(strings: Vec<String>) -> String {
    strings.join(", ")
}

#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_doubled_value",
    description = "An async Result function resolves with a doubled value for valid input.",
    exclude(
        java,
        reason = "The Java async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_overflow_for_negative_value",
    description = "An async Result function rejects negative input with the typed overflow error.",
    exclude(
        csharp,
        reason = "The C# async Result demo covers the zero-input invalid case instead of the negative overflow case."
    ),
    exclude(
        java,
        reason = "The Java async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_invalid_input_for_zero",
    description = "An async Result function rejects zero input with the typed invalid-input error.",
    exclude(
        apple,
        reason = "The Apple async Result demo covers the negative overflow case instead of the zero-input invalid case."
    ),
    exclude(
        java,
        reason = "The Java async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        wasm,
        reason = "The Wasm async Result demo covers the negative overflow case instead of the zero-input invalid case."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
pub async fn try_compute_async(value: i32) -> Result<i32, ComputeError> {
    crate::results::try_compute(value)
}

#[demo_bench_macros::demo_case(
    "async_fns.results.fetch_data.should_return_scaled_positive_id",
    description = "An async string-error Result function resolves with a scaled positive id.",
    exclude(
        java,
        reason = "The Java async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.fetch_data.should_reject_non_positive_id",
    description = "An async string-error Result function rejects a non-positive id.",
    exclude(
        java,
        reason = "The Java async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin async demo tests do not currently cover async Result helpers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
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
    description = "An async vector producer resolves with a zero-based counting sequence.",
    exclude(
        java,
        reason = "The Java async demo tests do not currently cover async_get_numbers."
    ),
    exclude(
        kotlin,
        reason = "The Kotlin async demo tests do not currently cover async_get_numbers."
    ),
    exclude(
        wasm,
        reason = "The Wasm async demo tests do not currently cover async_get_numbers."
    ),
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
#[demo_bench_macros::benchmark_candidate(function, uniffi)]
pub async fn async_get_numbers(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[demo_bench_macros::demo_case(
    "async_fns.mixed_record.echo.should_roundtrip_record",
    description = "An async function round-trips a mixed record containing nested records and enums.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
)]
#[export]
pub async fn async_echo_mixed_record(record: MixedRecord) -> MixedRecord {
    echo_mixed_record(record)
}

#[demo_bench_macros::demo_case(
    "async_fns.mixed_record.make.should_construct_record",
    description = "An async function constructs a mixed record from scalar, record, enum, and nested parameters.",
    exclude(
        python,
        reason = "The Python demo tests do not currently cover async functions."
    )
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
