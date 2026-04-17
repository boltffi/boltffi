#![allow(improper_ctypes_definitions)]
#![allow(clippy::unused_unit)]
#![allow(clippy::too_many_arguments)]

use std::sync::Mutex;

use boltffi::*;

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FixturePoint {
    pub x: f64,
    pub y: f64,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[repr(i32)]
pub enum FixtureStatus {
    #[default]
    Pending = 0,
    Active = 1,
    Completed = 2,
    Failed = 3,
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum FixtureTree {
    Leaf(i32),
    Node(Box<FixtureTree>, Box<FixtureTree>),
}

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
pub fn simple_maybe_double(x: i32) -> Option<i32> {
    if x > 0 { Some(x * 2) } else { None }
}

#[export]
pub fn simple_try_divide(a: i32, b: i32) -> Result<i32, i32> {
    if b != 0 { Ok(a / b) } else { Err(-1) }
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

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

struct YieldOnce(bool);

impl YieldOnce {
    fn new() -> Self {
        Self(false)
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[data]
#[derive(Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum FixtureError {
    NotFound = 1,
    InvalidInput = 2,
    Timeout = 3,
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found"),
            Self::InvalidInput => write!(f, "invalid input"),
            Self::Timeout => write!(f, "timeout"),
        }
    }
}

impl std::error::Error for FixtureError {}

#[export]
pub fn fallible_divide(a: i32, b: i32) -> Result<i32, FixtureError> {
    if b == 0 {
        Err(FixtureError::InvalidInput)
    } else {
        Ok(a / b)
    }
}

#[export]
pub fn fallible_lookup(key: i32) -> Result<String, FixtureError> {
    match key {
        1 => Ok("one".to_string()),
        2 => Ok("two".to_string()),
        3 => Ok("three".to_string()),
        _ => Err(FixtureError::NotFound),
    }
}

#[export]
pub async fn async_fallible_fetch(key: i32) -> Result<String, FixtureError> {
    tokio::time::sleep(Duration::from_millis(10)).await;
    if key < 0 {
        Err(FixtureError::InvalidInput)
    } else if key > 100 {
        Err(FixtureError::NotFound)
    } else {
        Ok(format!("value_{}", key))
    }
}

pub struct CancellableTask {
    started: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    iterations: Arc<AtomicU32>,
}

impl Default for CancellableTask {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl CancellableTask {
    pub fn new() -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
            completed: Arc::new(AtomicBool::new(false)),
            iterations: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn was_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    pub fn was_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    pub fn iteration_count(&self) -> u32 {
        self.iterations.load(Ordering::SeqCst)
    }

    pub async fn long_running_task(&self) -> i32 {
        self.started.store(true, Ordering::SeqCst);

        for i in 0..100 {
            self.iterations.store(i, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            YieldOnce::new().await;
        }

        self.completed.store(true, Ordering::SeqCst);
        42
    }

    pub async fn instant_task(&self) -> i32 {
        self.started.store(true, Ordering::SeqCst);
        self.completed.store(true, Ordering::SeqCst);
        99
    }
}

pub struct FallibleService {
    failure_mode: Arc<AtomicU32>,
}

impl Default for FallibleService {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl FallibleService {
    pub fn new() -> Self {
        Self {
            failure_mode: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn set_failure_mode(&self, mode: u32) {
        self.failure_mode.store(mode, Ordering::SeqCst);
    }

    pub fn get_value(&self, key: i32) -> Result<i32, FixtureError> {
        match self.failure_mode.load(Ordering::SeqCst) {
            1 => Err(FixtureError::NotFound),
            2 => Err(FixtureError::InvalidInput),
            3 => Err(FixtureError::Timeout),
            _ => Ok(key * 2),
        }
    }

    pub async fn async_get_value(&self, key: i32) -> Result<i32, FixtureError> {
        tokio::time::sleep(Duration::from_millis(5)).await;
        self.get_value(key)
    }

    pub fn get_optional(&self, key: i32) -> Option<i32> {
        if key > 0 { Some(key * 3) } else { None }
    }

    pub fn get_nested_result(&self, key: i32) -> Result<Option<i32>, FixtureError> {
        match self.failure_mode.load(Ordering::SeqCst) {
            1 => Err(FixtureError::NotFound),
            _ if key < 0 => Ok(None),
            _ => Ok(Some(key * 4)),
        }
    }
}

pub struct CounterStream {
    producer: StreamProducer<i32>,
}

impl Default for CounterStream {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl CounterStream {
    pub fn new() -> Self {
        Self {
            producer: StreamProducer::new(256),
        }
    }

    pub fn emit(&self, value: i32) {
        self.producer.push(value);
    }

    pub fn emit_batch(&self, values: Vec<i32>) -> u32 {
        values
            .iter()
            .map(|v| {
                self.producer.push(*v);
            })
            .count() as u32
    }

    #[ffi_stream(item = i32)]
    pub fn subscribe(&self) -> Arc<EventSubscription<i32>> {
        self.producer.subscribe()
    }
}

pub struct PointStream {
    producer: StreamProducer<FixturePoint>,
}

impl Default for PointStream {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl PointStream {
    pub fn new() -> Self {
        Self {
            producer: StreamProducer::new(32),
        }
    }

    pub fn emit(&self, point: FixturePoint) {
        self.producer.push(point);
    }

    #[ffi_stream(item = FixturePoint)]
    pub fn subscribe(&self) -> Arc<EventSubscription<FixturePoint>> {
        self.producer.subscribe()
    }
}

pub struct TestCounter {
    value: i32,
}

#[export(single_threaded)]
impl TestCounter {
    pub fn new(initial: i32) -> Self {
        Self { value: initial }
    }

    pub fn get(&self) -> i32 {
        self.value
    }

    pub fn set(&mut self, value: i32) {
        self.value = value;
    }

    pub fn add(&mut self, amount: i32) -> i32 {
        self.value += amount;
        self.value
    }

    pub async fn async_get(&self) -> i32 {
        self.value
    }

    pub async fn async_add(&mut self, amount: i32) -> i32 {
        self.value += amount;
        self.value
    }
}

pub struct ThreadSafeCounter {
    value: Mutex<i32>,
}

#[export]
impl ThreadSafeCounter {
    pub fn new(initial: i32) -> Self {
        Self {
            value: Mutex::new(initial),
        }
    }

    pub fn get(&self) -> i32 {
        *self.value.lock().unwrap()
    }

    pub fn set(&self, value: i32) {
        *self.value.lock().unwrap() = value;
    }

    pub fn add(&self, amount: i32) -> i32 {
        let mut guard = self.value.lock().unwrap();
        *guard += amount;
        *guard
    }

    pub fn increment(&self) -> i32 {
        self.add(1)
    }
}

pub struct ClassTestFixture {
    id: i32,
    name: String,
    point: FixturePoint,
    status: FixtureStatus,
    values: Vec<i32>,
    optional: Option<i32>,
}

#[export(single_threaded)]
impl ClassTestFixture {
    pub fn new_default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            point: FixturePoint::default(),
            status: FixtureStatus::Pending,
            values: Vec::new(),
            optional: None,
        }
    }

    pub fn new_with_id(id: i32) -> Self {
        Self {
            id,
            name: String::new(),
            point: FixturePoint::default(),
            status: FixtureStatus::Pending,
            values: Vec::new(),
            optional: None,
        }
    }

    pub fn new_with_name(name: String) -> Self {
        Self {
            id: 0,
            name,
            point: FixturePoint::default(),
            status: FixtureStatus::Pending,
            values: Vec::new(),
            optional: None,
        }
    }

    pub fn new_with_point(point: FixturePoint) -> Self {
        Self {
            id: 0,
            name: String::new(),
            point,
            status: FixtureStatus::Pending,
            values: Vec::new(),
            optional: None,
        }
    }

    pub fn new_with_status(status: FixtureStatus) -> Self {
        Self {
            id: 0,
            name: String::new(),
            point: FixturePoint::default(),
            status,
            values: Vec::new(),
            optional: None,
        }
    }

    pub fn new_full(id: i32, name: String, point: FixturePoint, status: FixtureStatus) -> Self {
        Self {
            id,
            name,
            point,
            status,
            values: Vec::new(),
            optional: None,
        }
    }

    pub fn try_new(id: i32) -> Result<Self, String> {
        if id < 0 {
            Err("id must be non-negative".to_string())
        } else {
            Ok(Self::new_with_id(id))
        }
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_point(&self) -> FixturePoint {
        self.point
    }

    pub fn get_status(&self) -> FixtureStatus {
        self.status
    }

    pub fn get_values(&self) -> Vec<i32> {
        self.values.clone()
    }

    pub fn get_optional(&self) -> Option<i32> {
        self.optional
    }

    pub fn set_id(&mut self, id: i32) {
        self.id = id;
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn set_point(&mut self, point: FixturePoint) {
        self.point = point;
    }

    pub fn set_status(&mut self, status: FixtureStatus) {
        self.status = status;
    }

    pub fn set_values(&mut self, values: Vec<i32>) {
        self.values = values;
    }

    pub fn set_optional(&mut self, optional: Option<i32>) {
        self.optional = optional;
    }

    pub fn add_value(&mut self, value: i32) {
        self.values.push(value);
    }

    pub fn clear_values(&mut self) {
        self.values.clear();
    }

    pub fn values_count(&self) -> i32 {
        self.values.len() as i32
    }

    pub fn compute_sum(&self) -> i32 {
        self.values.iter().sum()
    }

    pub fn try_get_value(&self, index: i32) -> Result<i32, String> {
        if index < 0 || index as usize >= self.values.len() {
            Err(format!("index {} out of bounds", index))
        } else {
            Ok(self.values[index as usize])
        }
    }

    pub fn find_value(&self, target: i32) -> Option<i32> {
        self.values
            .iter()
            .position(|&v| v == target)
            .map(|i| i as i32)
    }

    pub fn static_add(a: i32, b: i32) -> i32 {
        a.wrapping_add(b)
    }

    pub fn static_concat(a: String, b: String) -> String {
        format!("{}{}", a, b)
    }

    pub fn static_make_point(x: f64, y: f64) -> FixturePoint {
        FixturePoint { x, y }
    }

    pub fn static_identity_status(status: FixtureStatus) -> FixtureStatus {
        status
    }

    pub fn static_try_parse(s: String) -> Result<i32, String> {
        s.parse::<i32>().map_err(|e| e.to_string())
    }

    pub fn static_maybe_value(flag: bool) -> Option<i32> {
        if flag { Some(42) } else { None }
    }

    pub async fn async_get_id(&self) -> i32 {
        self.id
    }

    pub async fn async_get_name(&self) -> String {
        self.name.clone()
    }

    pub async fn async_set_id(&mut self, id: i32) {
        self.id = id;
    }

    pub async fn async_set_name(&mut self, name: String) {
        self.name = name;
    }

    pub async fn async_add_value(&mut self, value: i32) -> i32 {
        self.values.push(value);
        self.values.len() as i32
    }

    pub async fn async_compute_sum(&self) -> i32 {
        self.values.iter().sum()
    }

    pub async fn async_try_get(&self, index: i32) -> Result<i32, String> {
        if index < 0 || index as usize >= self.values.len() {
            Err(format!("index {} out of bounds", index))
        } else {
            Ok(self.values[index as usize])
        }
    }

    pub async fn async_find(&self, target: i32) -> Option<i32> {
        self.values
            .iter()
            .position(|&v| v == target)
            .map(|i| i as i32)
    }

    pub fn with_primitives(
        &self,
        a: i8,
        b: u8,
        c: i16,
        d: u16,
        e: i64,
        f: u64,
        g: f32,
        h: f64,
        i: bool,
    ) -> i64 {
        (a as i64)
            + (b as i64)
            + (c as i64)
            + (d as i64)
            + e
            + (f as i64)
            + (g as i64)
            + (h as i64)
            + (if i { 1 } else { 0 })
    }

    pub fn echo_bytes(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    pub fn values_near_point(&self, target: FixturePoint) -> Vec<i32> {
        let threshold = (target.x.abs() + target.y.abs()) as i32;
        self.values
            .iter()
            .copied()
            .filter(|&v| v.abs() <= threshold)
            .collect()
    }
}

#[data(impl)]
impl FixturePoint {
    pub fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    pub fn new_at(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to_origin(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn scale(&mut self, factor: f64) {
        self.x *= factor;
        self.y *= factor;
    }

    pub fn midpoint_to(a: FixturePoint, b: FixturePoint) -> FixturePoint {
        FixturePoint {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        }
    }
}
