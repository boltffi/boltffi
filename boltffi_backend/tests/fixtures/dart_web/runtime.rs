#[data]
pub struct Scores { pub values: Vec<i32> }
#[data]
pub struct Point { pub x: f64, pub y: f64 }
#[data]
pub struct Failure { pub message: String }
#[data]
pub struct Options {
    #[boltffi::default("hello")]
    pub name: String,
    #[boltffi::default(3)]
    pub count: u32,
}
#[export]
pub fn echo_scores(value: Scores) -> Scores { value }
#[export]
pub fn echo_values(value: Vec<i32>) -> Vec<i32> { value }
#[export]
pub fn echo_bools(value: Vec<bool>) -> Vec<bool> { value }
#[export]
pub fn echo_longs(value: Vec<i64>) -> Vec<i64> { value }
#[export]
pub fn mutate_point(value: &mut Point) {}
#[export]
pub fn mutate_fallible(value: &mut Point) -> Result<(), Failure> { todo!() }
#[export]
pub fn fallible() -> Result<i32, Failure> { todo!() }
#[export]
pub fn fail_string() -> Result<(), String> { todo!() }
#[export]
pub fn greeting(#[boltffi::default("world")] name: String) -> String { name }
#[export]
pub fn interleaved(before: i32, #[boltffi::default(1)] first: i32, middle: i32, #[boltffi::default(2)] last: i32) -> i32 { 0 }
#[export]
pub fn default_url(#[boltffi::default("https://example.com/")] value: url::Url) -> url::Url { value }
#[export]
pub fn default_uuid(#[boltffi::default("ffffffff-ffff-ffff-ffff-ffffffffffff")] value: uuid::Uuid) -> uuid::Uuid { value }
#[export]
pub async fn pending(callback: impl Fn()) -> i32 { todo!() }
#[export]
pub async fn maybe_value(flag: bool) -> Option<i32> { None }
#[export]
pub async fn async_fallible() -> Result<i32, Failure> { todo!() }
#[export]
pub fn echo_url(value: url::Url) -> url::Url { value }
#[export]
pub fn echo_uuid(value: uuid::Uuid) -> uuid::Uuid { value }
#[export]
pub fn echo_time(value: std::time::SystemTime) -> std::time::SystemTime { value }
#[export]
pub fn echo_duration(value: std::time::Duration) -> std::time::Duration { value }
#[export]
pub trait Adder {
    fn add(&self, left: i32, right: i32) -> i32;
}
#[export]
pub fn call_adder(adder: Box<dyn Adder>) -> i32 { todo!() }
pub struct Counter;
#[export]
impl Counter {
    pub fn new(value: i32) -> Self { Self }
    pub fn from_value(value: i32) -> Self { Self }
    pub fn value(&self) -> i32 { 0 }
    pub fn nullable(&self, a: Option<i32>, b: Option<i32>) -> i32 { 0 }
    #[ffi_stream(item = i32)]
    pub fn values(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }
    #[ffi_stream(item = i32, mode = "batch")]
    pub fn batches(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }
    #[ffi_stream(item = i32, mode = "callback")]
    pub fn notifications(&self) -> std::sync::Arc<boltffi::EventSubscription<i32>> { todo!() }
}

#[export]
pub async fn collision(#[boltffi::default(3)] cancellation_token: u32) -> u32 { cancellation_token }
