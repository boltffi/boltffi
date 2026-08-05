use boltffi::*;

/// A snapshot whose fields never cross the wire as a serialized layout.
///
/// The host receives one owned handle and reads each field through a
/// generated accessor, so the Rust value stays authoritative and is
/// destroyed exactly once when the host releases the handle.
#[data(opaque)]
#[derive(Clone, Debug)]
pub struct EngineSnapshot {
    pub revision: u32,
    pub label: String,
    pub build_tag: Option<String>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.native_opaque.engine_snapshot.should_read_fields_through_accessors",
    justification = "Ensure an opaque record returned by value exposes its primitive, string, and optional fields through generated accessors instead of a serialized layout.",
    directions = "Call `records::native_opaque::capture_engine_snapshot` through the generated binding and assert the returned handle reports the revision, label, and build tag through field accessors.",
    exclude(
        swift,
        reason = ExclusionReason::CoverageGap,
        details = "The Swift target generates ownership-safe opaque record wrappers, but the Swift demo suite does not exercise them yet. Add the marker when Swift demo coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "The Kotlin/JNI target generates ownership-safe opaque record wrappers, but the Kotlin demo suite does not exercise them yet. Add the marker when Kotlin demo coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "The Java target does not render native opaque records; opaque support is limited to the CPython, Swift, and Kotlin/JNI hosts."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::ImplementationGap,
        details = "The C# target does not render native opaque records; opaque support is limited to the CPython, Swift, and Kotlin/JNI hosts."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "Native opaque records hand the host an owned Rust handle, which the wasm/TypeScript host cannot own; unsupported hosts are gated before rendering."
    )
)]
pub fn capture_engine_snapshot(revision: u32) -> EngineSnapshot {
    EngineSnapshot {
        revision,
        label: format!("engine-{revision}"),
        build_tag: Some(format!("build-{revision}")),
    }
}

#[export]
#[demo_bench_macros::demo_case(
    "records.native_opaque.engine_snapshot.should_report_absent_optional_field",
    justification = "Ensure an absent one-level optional field on an opaque record reads back as the host's null value rather than a default.",
    directions = "Call `records::native_opaque::capture_untagged_snapshot` through the generated binding and assert the build tag accessor reports the host's absent value while the other fields still read back.",
    exclude(
        swift,
        reason = ExclusionReason::CoverageGap,
        details = "The Swift target generates ownership-safe opaque record wrappers, but the Swift demo suite does not exercise them yet. Add the marker when Swift demo coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "The Kotlin/JNI target generates ownership-safe opaque record wrappers, but the Kotlin demo suite does not exercise them yet. Add the marker when Kotlin demo coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::ImplementationGap,
        details = "The Java target does not render native opaque records; opaque support is limited to the CPython, Swift, and Kotlin/JNI hosts."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::ImplementationGap,
        details = "The C# target does not render native opaque records; opaque support is limited to the CPython, Swift, and Kotlin/JNI hosts."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "Native opaque records hand the host an owned Rust handle, which the wasm/TypeScript host cannot own; unsupported hosts are gated before rendering."
    )
)]
pub fn capture_untagged_snapshot(revision: u32) -> EngineSnapshot {
    EngineSnapshot {
        revision,
        label: format!("engine-{revision}"),
        build_tag: None,
    }
}
