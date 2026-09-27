use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::enums::c_style::Status;
use crate::records::blittable::{DataPoint, Point};
use crate::results::error_enums::MathError;

/// A callback trait for transforming integer values.
#[export]
pub trait ValueCallback {
    /// Called with an integer, returns a transformed integer.
    fn on_value(&self, value: i32) -> i32;
}

#[export]
pub fn invoke_value_callback(callback: impl ValueCallback, input: i32) -> i32 {
    callback.on_value(input)
}

#[export]
pub fn invoke_value_callback_twice(callback: impl ValueCallback, a: i32, b: i32) -> i32 {
    callback.on_value(a) + callback.on_value(b)
}

#[export]
pub fn invoke_boxed_value_callback(callback: Box<dyn ValueCallback>, input: i32) -> i32 {
    callback.on_value(input)
}

#[export]
pub fn invoke_optional_value_callback(callback: Option<Box<dyn ValueCallback>>, input: i32) -> i32 {
    callback
        .map(|value_callback| value_callback.on_value(input))
        .unwrap_or(input)
}

struct IncrementingValueCallback {
    delta: i32,
}

impl ValueCallback for IncrementingValueCallback {
    fn on_value(&self, value: i32) -> i32 {
        value + self.delta
    }
}

#[export]
pub fn make_incrementing_callback(delta: i32) -> Box<dyn ValueCallback> {
    Box::new(IncrementingValueCallback { delta })
}

#[export]
pub trait PointTransformer {
    fn transform(&self, point: Point) -> Point;
}

#[export]
pub fn transform_point(transformer: impl PointTransformer, point: Point) -> Point {
    transformer.transform(point)
}

#[export]
pub fn transform_point_boxed(transformer: Box<dyn PointTransformer>, point: Point) -> Point {
    transformer.transform(point)
}

#[export]
pub trait StatusMapper {
    fn map_status(&self, status: Status) -> Status;
}

#[export]
pub fn map_status(mapper: impl StatusMapper, status: Status) -> Status {
    mapper.map_status(status)
}

struct FlippingStatusMapper;

impl StatusMapper for FlippingStatusMapper {
    fn map_status(&self, status: Status) -> Status {
        match status {
            Status::Active => Status::Inactive,
            Status::Inactive => Status::Pending,
            Status::Pending => Status::Active,
        }
    }
}

#[export]
pub fn make_status_flipper() -> Box<dyn StatusMapper> {
    Box::new(FlippingStatusMapper)
}

#[export]
pub trait VecProcessor {
    fn process(&self, values: Vec<i32>) -> Vec<i32>;
}

#[export]
pub fn process_vec(processor: impl VecProcessor, values: Vec<i32>) -> Vec<i32> {
    processor.process(values)
}

#[export]
pub trait MessageFormatter {
    fn format_message(&self, scope: &str, message: &str) -> String;
}

#[export]
pub fn format_message_with_callback(
    formatter: impl MessageFormatter,
    scope: String,
    message: String,
) -> String {
    formatter.format_message(&scope, &message)
}

#[export]
pub fn format_message_with_boxed_callback(
    formatter: Box<dyn MessageFormatter>,
    scope: String,
    message: String,
) -> String {
    formatter.format_message(&scope, &message)
}

#[export]
pub fn format_message_with_optional_callback(
    formatter: Option<Box<dyn MessageFormatter>>,
    scope: String,
    message: String,
) -> String {
    formatter
        .map(|formatter| formatter.format_message(&scope, &message))
        .unwrap_or_else(|| format!("{scope}::{message}"))
}

struct PrefixingMessageFormatter {
    prefix: String,
}

impl MessageFormatter for PrefixingMessageFormatter {
    fn format_message(&self, scope: &str, message: &str) -> String {
        format!("{}::{scope}::{message}", self.prefix)
    }
}

#[export]
pub fn make_message_prefixer(prefix: String) -> Box<dyn MessageFormatter> {
    Box::new(PrefixingMessageFormatter { prefix })
}

#[export]
pub trait OptionalMessageCallback {
    fn find_message(&self, key: i32) -> Option<String>;
}

#[export]
pub fn invoke_optional_message_callback(
    callback: impl OptionalMessageCallback,
    key: i32,
) -> Option<String> {
    callback.find_message(key)
}

#[export]
pub trait ResultMessageCallback {
    fn render_message(&self, key: i32) -> Result<String, MathError>;
}

#[export]
pub fn invoke_result_message_callback(
    callback: impl ResultMessageCallback,
    key: i32,
) -> Result<String, MathError> {
    callback.render_message(key)
}

#[export]
pub trait StringResultMessageCallback {
    fn render_message(&self, key: i32) -> Result<String, String>;
}

#[demo_bench_macros::demo_case(
    "case:callbacks.sync_traits.string_result_message_callback.should_return_encoded_success",
    justification = "Ensure a callback method returning Result<String, String> returns its encoded success payload.",
    directions = "Call `callbacks::sync_traits::invoke_string_result_message_callback` through the generated binding and assert a callback Result<String, String> returns its encoded success payload."
,
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C target does not yet wrap callback result payloads ergonomically"
    )
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.sync_traits.string_result_message_callback.should_report_string_error",
    justification = "Ensure a callback method returning Result<String, String> reports its encoded String error payload.",
    directions = "Call `callbacks::sync_traits::invoke_string_result_message_callback` through the generated binding with a failing callback and assert the String error is reported by the target language."
,
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C target does not yet wrap callback result payloads ergonomically"
    )
)]
#[export]
pub fn invoke_string_result_message_callback(
    callback: impl StringResultMessageCallback,
    key: i32,
) -> Result<String, String> {
    callback.render_message(key)
}

#[export]
pub trait MultiMethodCallback {
    fn method_a(&self, x: i32) -> i32;
    fn method_b(&self, x: i32, y: i32) -> i32;
    fn method_c(&self) -> i32;
}

#[export]
pub fn invoke_multi_method(callback: impl MultiMethodCallback, x: i32, y: i32) -> i32 {
    callback.method_a(x) + callback.method_b(x, y) + callback.method_c()
}

#[export]
pub fn invoke_multi_method_boxed(callback: Box<dyn MultiMethodCallback>, x: i32, y: i32) -> i32 {
    callback.method_a(x) + callback.method_b(x, y) + callback.method_c()
}

#[export]
pub fn invoke_two_callbacks(
    first: impl ValueCallback,
    second: impl ValueCallback,
    value: i32,
) -> i32 {
    first.on_value(value) + second.on_value(value)
}

#[export]
pub trait OptionCallback {
    fn find_value(&self, key: i32) -> Option<i32>;
}

#[export]
pub fn invoke_option_callback(callback: impl OptionCallback, key: i32) -> Option<i32> {
    callback.find_value(key)
}

#[export]
pub trait ResultCallback {
    fn compute(&self, value: i32) -> Result<i32, MathError>;
}

#[export]
pub fn invoke_result_callback(callback: impl ResultCallback, value: i32) -> Result<i32, MathError> {
    callback.compute(value)
}

#[export]
pub trait FalliblePointTransformer {
    fn transform_point(&self, point: Point, status: Status) -> Result<Point, MathError>;
}

#[export]
pub fn invoke_fallible_point_transformer(
    callback: impl FalliblePointTransformer,
    point: Point,
    status: Status,
) -> Result<Point, MathError> {
    callback.transform_point(point, status)
}

#[export]
pub trait OffsetCallback {
    fn offset(&self, value: isize, delta: usize) -> isize;
}

#[export]
pub fn invoke_offset_callback(callback: impl OffsetCallback, value: isize, delta: usize) -> isize {
    callback.offset(value, delta)
}

#[export]
pub fn invoke_boxed_offset_callback(
    callback: Box<dyn OffsetCallback>,
    value: isize,
    delta: usize,
) -> isize {
    callback.offset(value, delta)
}

#[export]
#[benchmark_candidate(callback_interface, uniffi)]
pub trait DataProvider: Send + Sync {
    fn get_count(&self) -> u32;
    fn get_item(&self, index: u32) -> DataPoint;
}

#[benchmark_candidate(object, uniffi)]
pub struct DataConsumer {
    provider: std::sync::Mutex<Option<Box<dyn DataProvider>>>,
}

impl Default for DataConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
#[benchmark_candidate(impl, uniffi, constructor = "new")]
impl DataConsumer {
    pub fn new() -> Self {
        Self {
            provider: std::sync::Mutex::new(None),
        }
    }

    pub fn set_provider(&self, provider: Box<dyn DataProvider>) {
        *self.provider.lock().unwrap() = Some(provider);
    }

    pub fn compute_sum(&self) -> u64 {
        let provider_guard = self.provider.lock().unwrap();
        let Some(provider) = provider_guard.as_ref() else {
            return 0;
        };

        (0..provider.get_count())
            .map(|index| {
                let point = provider.get_item(index);
                (point.x + point.y) as u64
            })
            .sum()
    }
}
