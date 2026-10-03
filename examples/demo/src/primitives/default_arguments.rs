use std::convert::Infallible;
#[cfg(feature = "async-initializers")]
use std::sync::Arc;

use boltffi::*;

use crate::callbacks::sync_traits::ValueCallback;
use crate::custom_types::Email;

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults",
    justification = "A parameter's `#[boltffi::default(..)]` becomes a default argument, so a caller may leave trailing and named parameters out.",
    directions = "Call repeat_greeting with only a name and check the default greeting, count and case, then enable shout using named arguments or placeholders where available and explicit earlier values for positional APIs",
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
    justification = "An optional callback defaulting to None lets callers omit a handler when they do not have one",
    directions = "Call apply_optional_callback with only a value and check it is returned unchanged, then supply a doubling callback and check it is applied",
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub fn apply_optional_callback(
    value: i32,
    #[boltffi::default(None)] callback: Option<Box<dyn ValueCallback>>,
) -> i32 {
    callback.map_or(value, |callback| callback.on_value(value))
}

#[export]
pub fn apply_optional_closure(
    value: i32,
    #[boltffi::default(None)] callback: Option<Box<dyn Fn(i32) -> i32>>,
) -> i32 {
    callback.map_or(value, |callback| callback(value))
}

/// A counter whose start and step both have defaults.
pub struct DefaultedCounter {
    start: i32,
}

pub struct DefaultedWideCounter {
    value: i64,
}

#[export]
impl DefaultedWideCounter {
    pub fn new(#[boltffi::default(10)] value: i64) -> Self {
        Self { value }
    }

    pub fn value(&self) -> i64 {
        self.value
    }

    pub fn with_offset(counter: Self, #[boltffi::default(1)] step: i64) -> Self {
        Self {
            value: counter.value + step,
        }
    }
}

#[data]
#[derive(Clone, Copy)]
pub struct IntegerLimits {
    pub lower: i64,
    pub upper: u64,
}

#[export]
impl DefaultedCounter {
    pub fn from_text(#[boltffi::default("40")] start: String) -> Self {
        Self {
            start: start.parse().unwrap(),
        }
    }

    #[demo_bench_macros::demo_case(
        "primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults",
        justification = "Constructors and methods take parameter defaults the same way free functions do.",
        directions = "Construct `primitives::default_arguments::DefaultedCounter` without a start and call `offset` without a step; assert 10 + 1. Then construct with 5 and call `offset` with 3; assert 8.",
        exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
    )]
    pub fn new(#[boltffi::default(10)] start: i32) -> Self {
        Self { start }
    }

    pub fn offset(&self, #[boltffi::default(1)] step: i32) -> i32 {
        self.start + step
    }

    pub fn with_offset(#[boltffi::default(20)] start: i32, offset: i32) -> Self {
        Self {
            start: start + offset,
        }
    }

    pub async fn async_offset(&self, #[boltffi::default(3)] step: i32) -> i32 {
        self.start + step
    }

    #[cfg(feature = "async-initializers")]
    pub async fn start(
        #[boltffi::default(30)] start: i32,
        #[boltffi::default(None)] first: Option<Arc<dyn ValueCallback + Send + Sync>>,
        #[boltffi::default(None)] second: Option<Arc<dyn ValueCallback + Send + Sync>>,
    ) -> Self {
        let start = first.map_or(start, |callback| callback.on_value(start));
        let start = second.map_or(start, |callback| callback.on_value(start));
        Self { start }
    }

    pub fn integer_limits(
        #[boltffi::default(-9223372036854775808)] lower: i64,
        #[boltffi::default(18446744073709551615)] upper: u64,
    ) -> IntegerLimits {
        IntegerLimits { lower, upper }
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
#[data]
pub enum DefaultMode {
    Quiet = 1,
    Loud = 2,
}

#[derive(Clone, Copy)]
#[data]
pub enum DefaultChoice {
    Empty,
    Value(i32),
}

#[export]
pub fn choose_default(
    #[boltffi::default(DefaultChoice::Empty)] first: DefaultChoice,
    #[boltffi::default(None)] second: Option<DefaultChoice>,
) -> i32 {
    match (first, second) {
        (DefaultChoice::Value(value), _) => value,
        (DefaultChoice::Empty, Some(DefaultChoice::Value(value))) => -value,
        _ => 0,
    }
}

#[data(impl)]
impl DefaultMode {
    #[cfg(feature = "async-initializers")]
    pub async fn load(#[boltffi::default(DefaultMode::Quiet)] mode: Self) -> Self {
        mode
    }

    pub fn matches(&self, #[boltffi::default(DefaultMode::Quiet)] mode: Self) -> bool {
        *self as u8 == mode as u8
    }
}

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_apply_float_and_enum_defaults",
    justification = "Defaults retain floating point precision, optional values and enum identity across the native call",
    directions = "Call scale_default with omitted and explicit arguments, including a null weight, then check the default argument on DefaultMode::matches",
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub fn scale_default(
    #[boltffi::default(0.5)] ratio: f32,
    #[boltffi::default(1.5)] weight: Option<f64>,
    #[boltffi::default(DefaultMode::Quiet)] mode: DefaultMode,
) -> f64 {
    f64::from(ratio) * weight.unwrap_or(1.0) * f64::from(mode as u8)
}

#[export]
pub fn default_float_bits(#[boltffi::default(-0.0)] value: f32) -> u32 {
    value.to_bits()
}

#[export]
pub fn default_double_bits(#[boltffi::default(-0.0)] value: f64) -> u64 {
    value.to_bits()
}

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_apply_async_defaults",
    justification = "Async functions, methods and factories preserve parameter defaults",
    directions = "Call async_default and DefaultedCounter::async_offset with omitted and explicit values, then create a counter through start with omitted callbacks and with an explicit second callback",
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub async fn async_default(#[boltffi::default(9)] value: u32) -> u32 {
    value
}

#[data]
#[derive(Clone, Copy)]
pub struct DefaultAmount {
    #[boltffi::default(3)]
    pub value: i32,
}

#[data(impl)]
impl DefaultAmount {
    pub fn with_scaled_value(#[boltffi::default(2)] value: i32) -> Self {
        Self { value: value * 2 }
    }

    pub fn try_scaled_value(#[boltffi::default(2)] value: i32) -> Option<Self> {
        (value >= 0).then_some(Self { value: value * 2 })
    }

    #[demo_bench_macros::demo_case(
        "primitives.default_arguments.should_apply_record_defaults",
        justification = "Record fields and exported record callables each preserve their own defaults",
        directions = "Construct DefaultAmount through its memberwise initializer and call offset with omitted and explicit values, then call NamedAmount::with_value with and without its value",
        exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
    )]
    pub fn offset(&self, #[boltffi::default(2)] step: i32) -> i32 {
        self.value + step
    }
}

#[data]
#[derive(Clone, Copy)]
pub struct NamedAmount {
    pub value: i32,
}

#[data(impl)]
impl NamedAmount {
    #[cfg(feature = "async-initializers")]
    pub async fn load(#[boltffi::default(6)] value: i32) -> Self {
        Self { value }
    }

    pub fn with_value(#[boltffi::default(5)] value: i32) -> Self {
        Self { value }
    }
}

pub struct DefaultLimit(Option<u32>);

#[custom_ffi]
impl CustomFfiConvertible for DefaultLimit {
    type FfiRepr = Option<u32>;
    type Error = Infallible;

    fn into_ffi(&self) -> Self::FfiRepr {
        self.0
    }

    fn try_from_ffi(limit: Self::FfiRepr) -> Result<Self, Self::Error> {
        Ok(Self(limit))
    }
}

#[export]
pub fn default_limit(#[boltffi::default(None)] limit: DefaultLimit) -> Option<u32> {
    limit.0
}

#[demo_bench_macros::demo_case(
    "primitives.default_arguments.should_apply_custom_type_defaults",
    justification = "Parameter defaults follow custom types through record and optional representations",
    directions = "Call default_timeout_seconds without a timeout and with an explicit TimeoutFFI, then call default_limit without a limit and with a supplied value",
    exclude(c, reason = ExclusionReason::ImplementationGap, details = "C has no default arguments.")
)]
#[export]
pub fn default_timeout_seconds(#[boltffi::default(1.5)] timeout: chrono::TimeDelta) -> f64 {
    timeout.num_milliseconds() as f64 / 1_000.0
}

#[export]
pub fn default_email(#[boltffi::default("mailto:ada@example.com")] email: Email) -> Email {
    email
}

#[export]
pub fn default_optional_email(
    #[boltffi::default("mailto:ada@example.com")] email: Option<Email>,
) -> Option<Email> {
    email
}
