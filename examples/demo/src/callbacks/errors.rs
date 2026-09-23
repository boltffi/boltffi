use boltffi::*;

use crate::results::error_enums::MathError;

#[export]
pub trait FallibleWorker: Send + Sync {
    fn run(&self, mode: i32) -> Result<(), MathError>;
    fn value(&self, mode: i32) -> Result<i32, MathError>;
}

#[export]
#[allow(async_fn_in_trait)]
pub trait AsyncFallibleWorker: Send + Sync {
    async fn run(&self, mode: i32) -> Result<(), MathError>;
    async fn value(&self, mode: i32) -> Result<i32, MathError>;
}

#[demo_bench_macros::demo_case(
    "case:callbacks.errors.unit.should_report_success",
    justification = "A fallible unit callback returns successfully.",
    directions = "Call invoke_unit_worker with mode 0 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.unit.should_report_declared_error",
    justification = "A fallible unit callback preserves the declared error.",
    directions = "Call invoke_unit_worker with mode 1 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.unit.should_report_unexpected_error",
    justification = "A fallible unit callback converts an unexpected exception into the Rust error type.",
    directions = "Call invoke_unit_worker with mode 2 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[export]
pub fn invoke_unit_worker(worker: impl FallibleWorker, mode: i32) -> Result<(), MathError> {
    worker.run(mode)
}

#[demo_bench_macros::demo_case(
    "case:callbacks.errors.value.should_report_success",
    justification = "A fallible value callback returns successfully.",
    directions = "Call invoke_value_worker with mode 0 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.value.should_report_declared_error",
    justification = "A fallible value callback preserves the declared error.",
    directions = "Call invoke_value_worker with mode 1 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.value.should_report_unexpected_error",
    justification = "A fallible value callback converts an unexpected exception into the Rust error type.",
    directions = "Call invoke_value_worker with mode 2 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[export]
pub fn invoke_value_worker(worker: impl FallibleWorker, mode: i32) -> Result<i32, MathError> {
    worker.value(mode)
}

#[demo_bench_macros::demo_case(
    "case:callbacks.errors.async_unit.should_report_success",
    justification = "A fallible async unit callback returns successfully.",
    directions = "Call invoke_async_unit_worker with mode 0 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.async_unit.should_report_declared_error",
    justification = "A fallible async unit callback preserves the declared error.",
    directions = "Call invoke_async_unit_worker with mode 1 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.async_unit.should_report_unexpected_error",
    justification = "A fallible async unit callback converts an unexpected exception into the Rust error type.",
    directions = "Call invoke_async_unit_worker with mode 2 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[export]
pub async fn invoke_async_unit_worker(
    worker: impl AsyncFallibleWorker,
    mode: i32,
) -> Result<(), MathError> {
    worker.run(mode).await
}

#[demo_bench_macros::demo_case(
    "case:callbacks.errors.async_value.should_report_success",
    justification = "A fallible async value callback returns successfully.",
    directions = "Call invoke_async_value_worker with mode 0 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.async_value.should_report_declared_error",
    justification = "A fallible async value callback preserves the declared error.",
    directions = "Call invoke_async_value_worker with mode 1 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.errors.async_value.should_report_unexpected_error",
    justification = "A fallible async value callback converts an unexpected exception into the Rust error type.",
    directions = "Call invoke_async_value_worker with mode 2 and verify the result.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callbacks expose raw result payloads and do not throw host exceptions"
    )
)]
#[export]
pub async fn invoke_async_value_worker(
    worker: impl AsyncFallibleWorker,
    mode: i32,
) -> Result<i32, MathError> {
    worker.value(mode).await
}
