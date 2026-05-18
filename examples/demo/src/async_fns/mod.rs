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
    justification = "Ensure an async i32 addition function resolves with the sum.",
    directions = "Call `async_fns::async_add` through the generated binding and assert an async i32 addition function resolves with the sum.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
#[demo_bench_macros::benchmark_candidate(function, uniffi, wasm_bindgen)]
pub async fn async_add(a: i32, b: i32) -> i32 {
    a + b
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.echo.should_prefix_message",
    justification = "Ensure an async string function resolves with the expected prefixed message.",
    directions = "Call `async_fns::async_echo` through the generated binding and assert an async string function resolves with the expected prefixed message.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
pub async fn async_echo(message: String) -> String {
    format!("Echo: {}", message)
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.double_all.should_double_i32_vector",
    justification = "Ensure an async vector function resolves with every i32 value doubled.",
    directions = "Call `async_fns::async_double_all` through the generated binding and assert an async vector function resolves with every i32 value doubled.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
pub async fn async_double_all(values: Vec<i32>) -> Vec<i32> {
    values.into_iter().map(|v| v * 2).collect()
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.find_positive.should_return_first_positive",
    justification = "Ensure an async optional result resolves with the first positive i32 in a vector.",
    directions = "Call `async_fns::async_find_positive` through the generated binding and assert an async optional result resolves with the first positive i32 in a vector.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.basic.find_positive.should_return_none_for_all_negative",
    justification = "Ensure an async optional result resolves to none when no positive i32 is present.",
    directions = "Call `async_fns::async_find_positive` through the generated binding and assert an async optional result resolves to none when no positive i32 is present.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
pub async fn async_find_positive(values: Vec<i32>) -> Option<i32> {
    values.into_iter().find(|&v| v > 0)
}

#[demo_bench_macros::demo_case(
    "async_fns.basic.concat.should_join_string_vector",
    justification = "Ensure an async string-vector function resolves with the values joined by commas.",
    directions = "Call `async_fns::async_concat` through the generated binding and assert an async string-vector function resolves with the values joined by commas.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
pub async fn async_concat(strings: Vec<String>) -> String {
    strings.join(", ")
}

#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_doubled_value",
    justification = "Ensure an async Result function resolves with a doubled value for valid input.",
    directions = "Call `async_fns::try_compute_async` through the generated binding and assert an async Result function resolves with a doubled value for valid input.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_overflow_for_negative_value",
    justification = "Ensure an async Result function rejects negative input with the typed overflow error.",
    directions = "Call `async_fns::try_compute_async` through the generated binding and assert an async Result function rejects negative input with the typed overflow error.",
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "C# covers the zero-input invalid case for that branch today; add a separate assertion for the negative overflow case."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.try_compute.should_return_invalid_input_for_zero",
    justification = "Ensure an async Result function rejects zero input with the typed invalid-input error.",
    directions = "Call `async_fns::try_compute_async` through the generated binding and assert an async Result function rejects zero input with the typed invalid-input error.",
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript covers the negative overflow case for that branch today; add a separate assertion for the zero-input invalid case."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
pub async fn try_compute_async(value: i32) -> Result<i32, ComputeError> {
    crate::results::try_compute(value)
}

#[demo_bench_macros::demo_case(
    "async_fns.results.fetch_data.should_return_scaled_positive_id",
    justification = "Ensure an async string-error Result function resolves with a scaled positive id.",
    directions = "Call `async_fns::fetch_data` through the generated binding and assert an async string-error Result function resolves with a scaled positive id.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[demo_bench_macros::demo_case(
    "async_fns.results.fetch_data.should_reject_non_positive_id",
    justification = "Ensure an async string-error Result function rejects a non-positive id.",
    directions = "Call `async_fns::fetch_data` through the generated binding and assert an async string-error Result function rejects a non-positive id.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
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
    justification = "Ensure an async vector producer resolves with a zero-based counting sequence.",
    directions = "Call `async_fns::async_get_numbers` through the generated binding and assert an async vector producer resolves with a zero-based counting sequence.",
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "TypeScript has no assertion for async_get_numbers in the demo suite yet; add the marker at the scenario-specific test when coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
#[demo_bench_macros::benchmark_candidate(function, uniffi)]
pub async fn async_get_numbers(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[demo_bench_macros::demo_case(
    "async_fns.mixed_record.echo.should_roundtrip_record",
    justification = "Ensure an async function round-trips a mixed record containing nested records and enums.",
    directions = "Call `async_fns::async_echo_mixed_record` through the generated binding and assert an async function round-trips a mixed record containing nested records and enums.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
    )
)]
#[export]
pub async fn async_echo_mixed_record(record: MixedRecord) -> MixedRecord {
    echo_mixed_record(record)
}

#[demo_bench_macros::demo_case(
    "async_fns.mixed_record.make.should_construct_record",
    justification = "Ensure an async function constructs a mixed record from scalar, record, enum, and nested parameters.",
    directions = "Call `async_fns::async_make_mixed_record` through the generated binding and assert an async function constructs a mixed record from scalar, record, enum, and nested parameters.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python is experimental; its lowerer currently omits async functions. Include this case when async Python bindings are implemented."
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
