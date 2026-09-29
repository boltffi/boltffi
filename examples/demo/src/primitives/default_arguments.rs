use boltffi::*;

use crate::callbacks::sync_traits::ValueCallback;

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults",
    justification = "A parameter's `#[boltffi::default(..)]` becomes a default argument, so a caller may leave trailing and named parameters out.",
    directions = "Call `primitives::default_arguments::repeat_greeting` with only a name and assert it uses the default greeting, count and case; then pass only `shout` by name and assert the other defaults still apply.",
    exclude(swift, reason = ExclusionReason::ImplementationGap, details = "The Swift backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(java, reason = ExclusionReason::ImplementationGap, details = "The Java backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(csharp, reason = ExclusionReason::ImplementationGap, details = "The C# backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(typescript, reason = ExclusionReason::ImplementationGap, details = "The TypeScript backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(python, reason = ExclusionReason::ImplementationGap, details = "The Python extension parses positional arguments only and requires all of them."),
    exclude(dart, reason = ExclusionReason::ImplementationGap, details = "The Dart backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub fn repeat_greeting(
    name: String,
    #[boltffi::default("hello")] greeting: String,
    #[boltffi::default(2)] times: u32,
    #[boltffi::default(false)] shout: bool,
) -> String {
    let line = vec![format!("{greeting} {name}"); times as usize].join(", ");
    if shout { line.to_uppercase() } else { line }
}

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals",
    justification = "`None` is the natural default of an optional parameter, and a present default of an `Option<T>` is spelled as the `T`.",
    directions = "Call `primitives::default_arguments::describe_limit` with no arguments and assert it reports no label and the default limit of 7; then pass a label and assert it is used.",
    exclude(swift, reason = ExclusionReason::ImplementationGap, details = "The Swift backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(java, reason = ExclusionReason::ImplementationGap, details = "The Java backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(csharp, reason = ExclusionReason::ImplementationGap, details = "The C# backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(typescript, reason = ExclusionReason::ImplementationGap, details = "The TypeScript backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(python, reason = ExclusionReason::ImplementationGap, details = "The Python extension parses positional arguments only and requires all of them."),
    exclude(dart, reason = ExclusionReason::ImplementationGap, details = "The Dart backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub fn describe_limit(
    #[boltffi::default(None)] label: Option<String>,
    #[boltffi::default(7)] limit: Option<u16>,
) -> String {
    let label = label.unwrap_or_else(|| "none".to_owned());
    match limit {
        Some(limit) => format!("{label}:{limit}"),
        None => format!("{label}:unlimited"),
    }
}

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_default_an_optional_callback_to_none",
    justification = "An optional callback parameter defaulting to `None` lets a caller pass only the handlers it has, by name.",
    directions = "Call `primitives::default_arguments::apply_optional_callback` with only a value and assert it is returned unchanged; then pass a doubling callback by name and assert it is applied.",
    exclude(swift, reason = ExclusionReason::ImplementationGap, details = "The Swift backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(java, reason = ExclusionReason::ImplementationGap, details = "The Java backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(csharp, reason = ExclusionReason::ImplementationGap, details = "The C# backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(typescript, reason = ExclusionReason::ImplementationGap, details = "The TypeScript backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(python, reason = ExclusionReason::ImplementationGap, details = "The Python extension parses positional arguments only and requires all of them."),
    exclude(dart, reason = ExclusionReason::ImplementationGap, details = "The Dart backend renders parameters without their Rust defaults, so every argument is required."),
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub fn apply_optional_callback(
    value: i32,
    #[boltffi::default(None)] callback: Option<Box<dyn ValueCallback>>,
) -> i32 {
    callback.map_or(value, |callback| callback.on_value(value))
}

/// A counter whose start and step both have defaults.
pub struct DefaultedCounter {
    start: i32,
}

#[export]
impl DefaultedCounter {
    #[demo_bench_macros::demo_case(
        "primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults",
        justification = "Constructors and methods take parameter defaults the same way free functions do.",
        directions = "Construct `primitives::default_arguments::DefaultedCounter` without a start and call `offset` without a step; assert 10 + 1. Then construct with 5 and call `offset` with 3; assert 8.",
        exclude(swift, reason = ExclusionReason::ImplementationGap, details = "The Swift backend renders parameters without their Rust defaults, so every argument is required."),
        exclude(java, reason = ExclusionReason::ImplementationGap, details = "The Java backend renders parameters without their Rust defaults, so every argument is required."),
        exclude(csharp, reason = ExclusionReason::ImplementationGap, details = "The C# backend renders parameters without their Rust defaults, so every argument is required."),
        exclude(typescript, reason = ExclusionReason::ImplementationGap, details = "The TypeScript backend renders parameters without their Rust defaults, so every argument is required."),
        exclude(python, reason = ExclusionReason::ImplementationGap, details = "The Python extension parses positional arguments only and requires all of them."),
        exclude(dart, reason = ExclusionReason::ImplementationGap, details = "The Dart backend renders parameters without their Rust defaults, so every argument is required."),
        exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
    )]
    pub fn new(#[boltffi::default(10)] start: i32) -> Self {
        Self { start }
    }

    pub fn offset(&self, #[boltffi::default(1)] step: i32) -> i32 {
        self.start + step
    }
}
