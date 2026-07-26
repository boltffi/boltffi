use boltffi::*;

#[demo_bench_macros::demo_case(
    "constants.values.should_expose_inline_and_accessor_values",
    justification = "Ensure TypeScript exposes every supported constant delivery form after WASM initialization.",
    directions = "Read every generated inline and accessor-backed constant and assert its value.",
    exercises = [
        "constants::DEMO_ENABLED",
        "constants::DEMO_ANSWER",
        "constants::DEMO_LARGE",
        "constants::DEMO_HALF",
        "constants::DEMO_LABEL",
        "constants::DEMO_BYTES",
        "constants::DEMO_MODE",
        "constants::DEMO_IDLE",
        "constants::DEMO_ALIAS",
        "constants::DEMO_COMPUTED",
        "constants::DEMO_PAIR",
        "constants::DEMO_BUSY"
    ],
    exclude(
        swift,
        reason = ExclusionReason::CoverageGap,
        details = "The Swift demo suite does not assert exported constants yet. Add the marker when Swift constant coverage lands."
    ),
    exclude(
        kotlin,
        reason = ExclusionReason::CoverageGap,
        details = "The Kotlin demo suite does not assert exported constants yet. Add the marker when Kotlin constant coverage lands."
    ),
    exclude(
        java,
        reason = ExclusionReason::CoverageGap,
        details = "The Java demo suite does not assert exported constants yet. Add the marker when Java constant coverage lands."
    ),
    exclude(
        csharp,
        reason = ExclusionReason::CoverageGap,
        details = "The C# demo suite does not assert exported constants yet. Add the marker when C# constant coverage lands."
    ),
    exclude(
        python,
        reason = ExclusionReason::CoverageGap,
        details = "The Python demo suite does not assert exported constants yet. Add the marker when Python constant coverage lands."
    )
)]
#[export]
pub const DEMO_ENABLED: bool = true;

#[export]
pub const DEMO_ANSWER: u32 = 42;

#[export]
pub const DEMO_LARGE: i64 = 9_007_199_254_740_993;

#[export]
pub const DEMO_HALF: f64 = 0.5;

#[export]
pub const DEMO_LABEL: &str = "boltffi";

#[export]
pub const DEMO_BYTES: &'static [u8] = b"ffi";

#[data]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoMode {
    Fast = 1,
    Safe = 2,
}

#[export]
pub const DEMO_MODE: DemoMode = DemoMode::Fast;

#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DemoState {
    Idle,
    Busy { jobs: u32 },
}

#[export]
pub const DEMO_IDLE: DemoState = DemoState::Idle;

#[export]
pub const DEMO_ALIAS: &str = DEMO_LABEL;

#[export]
pub const DEMO_COMPUTED: u32 = 6 * 7;

#[export]
pub const DEMO_PAIR: (u32, u32) = (3, 5);

#[export]
pub const DEMO_BUSY: DemoState = DemoState::Busy { jobs: 3 };
