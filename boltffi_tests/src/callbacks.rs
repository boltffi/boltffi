use std::sync::Arc;

use boltffi::*;

use crate::{FixtureMessageRecord, FixturePoint, FixtureStatus};

#[export]
pub trait SyncValueCallback {
    fn on_value(&self, value: i32) -> i32;
}

#[export]
pub trait SyncDataProvider {
    fn get_count(&self) -> u32;
    fn get_item(&self, index: u32) -> FixturePoint;
}

#[export]
pub trait SyncVecCallback {
    fn on_vec(&self, values: Vec<i32>) -> Vec<i32>;
}

#[export]
pub trait SyncStructCallback {
    fn on_struct(&self, point: FixturePoint) -> FixturePoint;
}

#[export]
pub trait SyncOptionCallback {
    fn find_value(&self, key: i32) -> Option<i32>;
}

#[export]
pub trait SyncEnumCallback {
    fn get_status(&self, id: i32) -> FixtureStatus;
}

#[export]
pub trait SyncMultiMethodCallback {
    fn method_a(&self, x: i32) -> i32;
    fn method_b(&self, x: i32, y: i32) -> i32;
    fn method_c(&self) -> i32;
}

struct OffsetCallback {
    base: i32,
}

impl SyncValueCallback for OffsetCallback {
    fn on_value(&self, value: i32) -> i32 {
        self.base + value
    }
}

#[export]
#[allow(async_fn_in_trait)]
pub trait AsyncFetcher {
    async fn fetch(&self, key: u32) -> u64;
}

#[export]
#[allow(async_fn_in_trait, improper_ctypes_definitions)]
pub trait AsyncOptionFetcher {
    async fn find(&self, key: i32) -> Option<i64>;
}

#[export]
#[allow(async_fn_in_trait)]
pub trait AsyncMultiMethod {
    async fn load(&self, id: i64) -> i64;
    async fn compute(&self, a: i32, b: i32) -> i64;
}

#[export]
pub fn invoke_sync_boxed(callback: Box<dyn SyncValueCallback>, input: i32) -> i32 {
    callback.on_value(input)
}

#[export]
pub fn invoke_sync_impl(callback: impl SyncValueCallback, input: i32) -> i32 {
    callback.on_value(input)
}

#[export]
pub fn sum_provider_boxed(provider: Box<dyn SyncDataProvider>) -> f64 {
    let count = provider.get_count();
    (0..count).fold(0.0, |acc, i| {
        let point = provider.get_item(i);
        acc + point.x + point.y
    })
}

#[export]
pub fn sum_provider_impl(provider: impl SyncDataProvider) -> f64 {
    let count = provider.get_count();
    (0..count).fold(0.0, |acc, i| {
        let point = provider.get_item(i);
        acc + point.x + point.y
    })
}

#[export]
pub fn invoke_vec_boxed(callback: Box<dyn SyncVecCallback>, values: Vec<i32>) -> Vec<i32> {
    callback.on_vec(values)
}

#[export]
pub fn invoke_vec_impl(callback: impl SyncVecCallback, values: Vec<i32>) -> Vec<i32> {
    callback.on_vec(values)
}

#[export]
pub fn invoke_struct_boxed(
    callback: Box<dyn SyncStructCallback>,
    point: FixturePoint,
) -> FixturePoint {
    callback.on_struct(point)
}

#[export]
pub fn invoke_struct_impl(callback: impl SyncStructCallback, point: FixturePoint) -> FixturePoint {
    callback.on_struct(point)
}

#[export]
pub fn invoke_option_boxed(callback: Box<dyn SyncOptionCallback>, key: i32) -> Option<i32> {
    callback.find_value(key)
}

#[export]
pub fn invoke_option_impl(callback: impl SyncOptionCallback, key: i32) -> Option<i32> {
    callback.find_value(key)
}

#[export]
pub fn invoke_enum_boxed(callback: Box<dyn SyncEnumCallback>, id: i32) -> FixtureStatus {
    callback.get_status(id)
}

#[export]
pub fn invoke_enum_impl(callback: impl SyncEnumCallback, id: i32) -> FixtureStatus {
    callback.get_status(id)
}

#[export]
pub fn invoke_multi_method_boxed(
    callback: Box<dyn SyncMultiMethodCallback>,
    x: i32,
    y: i32,
) -> i32 {
    callback.method_a(x) + callback.method_b(x, y) + callback.method_c()
}

#[export]
pub fn invoke_multi_method_impl(callback: impl SyncMultiMethodCallback, x: i32, y: i32) -> i32 {
    callback.method_a(x) + callback.method_b(x, y) + callback.method_c()
}

#[export]
pub fn invoke_two_sync_impl(
    first: impl SyncValueCallback,
    second: impl SyncValueCallback,
    value: i32,
) -> i32 {
    first.on_value(value) + second.on_value(value)
}

#[export]
pub fn invoke_three_sync_impl(
    first: impl SyncValueCallback,
    second: impl SyncValueCallback,
    third: impl SyncValueCallback,
    value: i32,
) -> i32 {
    first.on_value(value) + second.on_value(value) + third.on_value(value)
}

#[export]
pub fn invoke_mixed_sync(
    boxed: Box<dyn SyncValueCallback>,
    impl_cb: impl SyncValueCallback,
    value: i32,
) -> i32 {
    boxed.on_value(value) * impl_cb.on_value(value)
}

#[export]
pub fn invoke_mixed_three(
    boxed: Box<dyn SyncValueCallback>,
    impl1: impl SyncValueCallback,
    impl2: impl SyncValueCallback,
    value: i32,
) -> i32 {
    boxed.on_value(value) + impl1.on_value(value) + impl2.on_value(value)
}

#[export]
pub fn make_value_callback(base: i32) -> Box<dyn SyncValueCallback> {
    Box::new(OffsetCallback { base })
}

#[export]
pub fn maybe_callback(present: bool) -> Option<Box<dyn SyncValueCallback>> {
    present.then(|| Box::new(OffsetCallback { base: 7 }) as Box<dyn SyncValueCallback>)
}

#[export]
pub fn shared_callback() -> Arc<dyn SyncValueCallback> {
    Arc::new(OffsetCallback { base: 11 })
}

#[export]
pub fn try_make_callback(fail: bool) -> Result<Box<dyn SyncValueCallback>, String> {
    if fail {
        Err("callback unavailable".to_string())
    } else {
        Ok(Box::new(OffsetCallback { base: 13 }))
    }
}

#[export]
pub async fn invoke_async_impl(fetcher: impl AsyncFetcher, key: u32) -> u64 {
    fetcher.fetch(key).await
}

#[export]
pub async fn invoke_two_async_impl(
    first: impl AsyncFetcher,
    second: impl AsyncFetcher,
    key: u32,
) -> u64 {
    first.fetch(key).await.wrapping_mul(second.fetch(key).await)
}

#[export]
pub async fn invoke_three_async_impl(
    first: impl AsyncFetcher,
    second: impl AsyncFetcher,
    third: impl AsyncFetcher,
    key: u32,
) -> u64 {
    first.fetch(key).await + second.fetch(key).await + third.fetch(key).await
}

#[export]
pub async fn invoke_async_option_impl(fetcher: impl AsyncOptionFetcher, key: i32) -> Option<i64> {
    fetcher.find(key).await
}

#[export]
pub async fn async_echo_message_record(record: FixtureMessageRecord) -> FixtureMessageRecord {
    record
}

#[export]
pub async fn invoke_async_multi_impl(
    callback: impl AsyncMultiMethod,
    id: i64,
    a: i32,
    b: i32,
) -> i64 {
    callback.load(id).await + callback.compute(a, b).await
}

pub struct SyncProcessor {
    multiplier: i32,
}

#[export]
impl SyncProcessor {
    pub fn new(multiplier: i32) -> Self {
        Self { multiplier }
    }

    pub fn apply_impl(&self, callback: impl SyncValueCallback, value: i32) -> i32 {
        callback.on_value(value * self.multiplier)
    }

    pub fn apply_boxed(&self, callback: Box<dyn SyncValueCallback>, value: i32) -> i32 {
        callback.on_value(value * self.multiplier)
    }

    pub fn apply_struct_impl(
        &self,
        callback: impl SyncStructCallback,
        point: FixturePoint,
    ) -> FixturePoint {
        let scaled = FixturePoint {
            x: point.x * self.multiplier as f64,
            y: point.y * self.multiplier as f64,
        };
        callback.on_struct(scaled)
    }

    pub fn apply_option_impl(&self, callback: impl SyncOptionCallback, key: i32) -> Option<i32> {
        callback.find_value(key * self.multiplier)
    }
}

pub struct AsyncProcessor {
    offset: u64,
}

#[export]
impl AsyncProcessor {
    pub fn new(offset: u64) -> Self {
        Self { offset }
    }

    pub async fn fetch_with_offset(&self, fetcher: impl AsyncFetcher, key: u32) -> u64 {
        fetcher.fetch(key).await.wrapping_add(self.offset)
    }

    pub async fn find_with_offset(
        &self,
        fetcher: impl AsyncOptionFetcher,
        key: i32,
    ) -> Option<i64> {
        fetcher.find(key).await.map(|v| v + self.offset as i64)
    }
}

#[export]
#[allow(async_fn_in_trait)]
pub trait AsyncCallbackFactory {
    async fn make(&self) -> Box<dyn SyncValueCallback>;
}

#[export]
pub async fn invoke_async_factory_impl(factory: impl AsyncCallbackFactory, input: i32) -> i32 {
    let callback = factory.make().await;
    callback.on_value(input)
}
