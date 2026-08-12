use boltffi::*;

#[demo_bench_macros::demo_case(
    "callbacks.closures.direct_vector_parameter.should_pass_values",
    justification = "Ensure a C# closure can receive a primitive vector through the direct-vector ABI.",
    directions = "Call `callbacks::apply_vector_closure` with a closure that sums the received values and assert the returned sum.",
    exclude(
        swift,
        reason = ExclusionReason::CoverageGap,
        details = "This regression fixture is currently enabled only for the C# demo."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "This regression fixture is currently enabled only for the C# demo."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "This regression fixture is currently enabled only for the C# demo."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::CoverageGap,
        details = "This regression fixture is currently enabled only for the C# demo."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "This regression fixture is currently enabled only for the C# demo."
    ),
    exclude(
        dart,
        reason = ExclusionReason::ImplementationGap,
        details = "The native Dart target does not emit a binding for a closure parameter accepting a direct-vector Vec<i32> argument at all -- apply_vector_closure doesn't appear in the generated output, so there is nothing to call."
    )
)]
#[export]
pub fn apply_vector_closure(f: impl Fn(Vec<i32>) -> i32, values: Vec<i32>) -> i32 {
    f(values)
}
