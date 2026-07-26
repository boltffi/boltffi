use std::collections::HashMap;

use boltffi::*;

#[demo_bench_macros::demo_case(
    "collections.hash_map.should_return_values",
    justification = "Ensure a Rust HashMap return becomes a TypeScript Map with every key and value intact.",
    directions = "Call `collections::make_hash_map` through the generated binding and assert the returned map contains the expected entries.",
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "The Kotlin demo suite does not assert HashMap returns yet. Add the marker when Kotlin map coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "The Java demo suite does not assert HashMap returns yet. Add the marker when Java map coverage lands."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "The C# demo suite does not assert HashMap returns yet. Add the marker when C# map coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "The Python demo suite does not assert HashMap returns yet. Add the marker when Python map coverage lands."
    )
)]
#[export]
pub fn make_hash_map() -> HashMap<String, i32> {
    [("first".to_owned(), 10), ("second".to_owned(), 20)]
        .into_iter()
        .collect()
}

#[demo_bench_macros::demo_case(
    "collections.hash_map.should_roundtrip_empty",
    justification = "Ensure an empty TypeScript Map crosses into Rust and returns without gaining entries.",
    directions = "Call `collections::echo_hash_map` with an empty map and assert the returned map remains empty.",
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "The Kotlin demo suite does not assert HashMap parameters yet. Add the marker when Kotlin map coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "The Java demo suite does not assert HashMap parameters yet. Add the marker when Java map coverage lands."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "The C# demo suite does not assert HashMap parameters yet. Add the marker when C# map coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "The Python demo suite does not assert HashMap parameters yet. Add the marker when Python map coverage lands."
    )
)]
#[demo_bench_macros::demo_case(
    "collections.hash_map.should_roundtrip_nested_values",
    justification = "Ensure HashMap values use their nested vector codec in both directions.",
    directions = "Call `collections::echo_hash_map` with string keys and vector values and assert every nested value returns intact.",
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "The Kotlin demo suite does not assert nested HashMap values yet. Add the marker when Kotlin map coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "The Java demo suite does not assert nested HashMap values yet. Add the marker when Java map coverage lands."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "The C# demo suite does not assert nested HashMap values yet. Add the marker when C# map coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "The Python demo suite does not assert nested HashMap values yet. Add the marker when Python map coverage lands."
    )
)]
#[export]
pub fn echo_hash_map(values: HashMap<String, Vec<i32>>) -> HashMap<String, Vec<i32>> {
    values
}
