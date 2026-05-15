use demo_bench_macros::benchmark_candidate;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

use crate::callbacks::sync_traits::{DataConsumer as DemoDataConsumer, DataProvider};
use crate::classes::methods::Counter as DemoCounter;
use crate::classes::thread_safe::Accumulator as DemoAccumulator;
use crate::classes::thread_safe::DataStore as DemoDataStore;
use crate::classes::unsafe_single_threaded::{
    AccumulatorSingleThreaded as DemoAccumulatorSingleThreaded,
    CounterSingleThreaded as DemoCounterSingleThreaded,
};
use crate::enums::data_enum::TaskStatus;
use crate::records::blittable::{DataPoint, Location};
use crate::records::with_collections::BenchmarkUserProfile;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserProfilePayload {
    id: i64,
    name: String,
    email: String,
    bio: String,
    age: i32,
    score: f64,
    tags: Vec<String>,
    scores: Vec<i32>,
    is_active: bool,
}

impl From<BenchmarkUserProfile> for UserProfilePayload {
    fn from(profile: BenchmarkUserProfile) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            email: profile.email,
            bio: profile.bio,
            age: profile.age,
            score: profile.score,
            tags: profile.tags,
            scores: profile.scores,
            is_active: profile.is_active,
        }
    }
}

impl From<UserProfilePayload> for BenchmarkUserProfile {
    fn from(profile: UserProfilePayload) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            email: profile.email,
            bio: profile.bio,
            age: profile.age,
            score: profile.score,
            tags: profile.tags,
            scores: profile.scores,
            is_active: profile.is_active,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "tag")]
enum TaskStatusPayload {
    Pending,
    InProgress {
        progress: i32,
    },
    Completed {
        result: i32,
    },
    Failed {
        #[serde(rename = "errorCode")]
        error_code: i32,
        #[serde(rename = "retryCount")]
        retry_count: i32,
    },
}

impl From<TaskStatusPayload> for TaskStatus {
    fn from(status: TaskStatusPayload) -> Self {
        match status {
            TaskStatusPayload::Pending => Self::Pending,
            TaskStatusPayload::InProgress { progress } => Self::InProgress { progress },
            TaskStatusPayload::Completed { result } => Self::Completed { result },
            TaskStatusPayload::Failed {
                error_code,
                retry_count,
            } => Self::Failed {
                error_code,
                retry_count,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DataPointPayload {
    x: f64,
    y: f64,
    timestamp: i64,
}

impl From<DataPointPayload> for DataPoint {
    fn from(point: DataPointPayload) -> Self {
        Self {
            x: point.x,
            y: point.y,
            timestamp: point.timestamp,
        }
    }
}

fn deserialize_js<T>(value: JsValue, label: &str) -> T
where
    T: DeserializeOwned,
{
    serde_wasm_bindgen::from_value(value).unwrap_or_else(|error| panic!("invalid {label}: {error}"))
}

fn serialize_js<T>(value: &T, label: &str) -> JsValue
where
    T: Serialize,
{
    serde_wasm_bindgen::to_value(value)
        .unwrap_or_else(|error| panic!("failed to serialize {label}: {error}"))
}

#[wasm_bindgen]
extern "C" {
    pub type JsDataProvider;

    #[wasm_bindgen(method, js_name = getCount)]
    fn get_count(this: &JsDataProvider) -> u32;

    #[wasm_bindgen(method, js_name = getItem)]
    fn get_item(this: &JsDataProvider, index: u32) -> JsValue;
}

struct DataProviderBridge {
    provider: JsValue,
}

impl DataProviderBridge {
    fn new(provider: JsValue) -> Self {
        Self { provider }
    }
}

unsafe impl Send for DataProviderBridge {}
unsafe impl Sync for DataProviderBridge {}

impl DataProvider for DataProviderBridge {
    fn get_count(&self) -> u32 {
        self.provider.unchecked_ref::<JsDataProvider>().get_count()
    }

    fn get_item(&self, index: u32) -> DataPoint {
        let item = self
            .provider
            .unchecked_ref::<JsDataProvider>()
            .get_item(index);
        let point: DataPointPayload = deserialize_js(item, "data point");
        point.into()
    }
}

#[benchmark_candidate(object, wasm_bindgen)]
pub struct DataStore {
    inner: DemoDataStore,
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl DataStore {
    pub fn new() -> Self {
        Self {
            inner: DemoDataStore::new(),
        }
    }

    pub fn add(&self, point: DataPoint) {
        self.inner.add(point);
    }

    pub fn add_parts(&self, x: f64, y: f64, timestamp: i64) {
        self.inner.add_parts(x, y, timestamp);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn sum(&self) -> f64 {
        self.inner.sum()
    }
}

#[wasm_bindgen(js_name = Counter)]
pub struct WasmBenchCounter {
    inner: DemoCounter,
}

#[wasm_bindgen(js_class = Counter)]
impl WasmBenchCounter {
    #[wasm_bindgen(constructor)]
    pub fn new(initial: i32) -> Self {
        Self {
            inner: DemoCounter::new(initial),
        }
    }

    pub fn get(&self) -> i32 {
        self.inner.get()
    }

    pub fn increment(&self) {
        self.inner.increment();
    }

    pub fn add(&self, amount: i32) {
        self.inner.add(amount);
    }

    pub fn reset(&self) {
        self.inner.reset();
    }
}

#[wasm_bindgen(js_name = Accumulator)]
pub struct WasmBenchAccumulator {
    inner: DemoAccumulator,
}

#[wasm_bindgen(js_class = Accumulator)]
impl WasmBenchAccumulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: DemoAccumulator::new(),
        }
    }

    pub fn add(&self, amount: i64) {
        self.inner.add(amount);
    }

    pub fn get(&self) -> i64 {
        self.inner.get()
    }

    pub fn reset(&self) {
        self.inner.reset();
    }
}

#[wasm_bindgen(js_name = CounterSingleThreaded)]
pub struct WasmBenchCounterSingleThreaded {
    inner: DemoCounterSingleThreaded,
}

#[wasm_bindgen(js_class = CounterSingleThreaded)]
impl WasmBenchCounterSingleThreaded {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: DemoCounterSingleThreaded::new(),
        }
    }

    pub fn increment(&mut self) {
        self.inner.increment();
    }

    pub fn get(&self) -> i32 {
        self.inner.get()
    }
}

#[wasm_bindgen(js_name = AccumulatorSingleThreaded)]
pub struct WasmBenchAccumulatorSingleThreaded {
    inner: DemoAccumulatorSingleThreaded,
}

#[wasm_bindgen(js_class = AccumulatorSingleThreaded)]
impl WasmBenchAccumulatorSingleThreaded {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: DemoAccumulatorSingleThreaded::new(),
        }
    }

    pub fn add(&mut self, amount: i64) {
        self.inner.add(amount);
    }

    pub fn get(&self) -> i64 {
        self.inner.get()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

#[benchmark_candidate(object, wasm_bindgen)]
pub struct DataConsumer {
    inner: DemoDataConsumer,
}

#[benchmark_candidate(impl, wasm_bindgen, constructor = "new")]
impl DataConsumer {
    pub fn new() -> Self {
        Self {
            inner: DemoDataConsumer::new(),
        }
    }

    pub fn set_provider(&self, provider: JsValue) {
        self.inner
            .set_provider(Box::new(DataProviderBridge::new(provider)));
    }

    pub fn compute_sum(&self) -> u64 {
        self.inner.compute_sum()
    }
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn inc_u64(mut values: Vec<u64>) -> Vec<u64> {
    crate::inc_u64(&mut values);
    values
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn find_numbers(count: i32) -> Vec<i32> {
    crate::find_numbers(count).unwrap_or_default()
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn find_names(count: i32) -> Vec<String> {
    crate::find_names(count).unwrap_or_default()
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn find_locations(count: i32) -> Vec<Location> {
    crate::find_locations(count).unwrap_or_default()
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn generate_user_profiles(count: i32) -> JsValue {
    let profiles: Vec<UserProfilePayload> = crate::generate_user_profiles(count)
        .into_iter()
        .map(UserProfilePayload::from)
        .collect();
    serialize_js(&profiles, "user profiles")
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn sum_user_scores(users: JsValue) -> f64 {
    let profiles: Vec<UserProfilePayload> = deserialize_js(users, "user profiles");
    crate::sum_user_scores(
        profiles
            .into_iter()
            .map(BenchmarkUserProfile::from)
            .collect(),
    )
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn count_active_users(users: JsValue) -> i32 {
    let profiles: Vec<UserProfilePayload> = deserialize_js(users, "user profiles");
    crate::count_active_users(
        profiles
            .into_iter()
            .map(BenchmarkUserProfile::from)
            .collect(),
    )
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn get_status_progress(status: JsValue) -> i32 {
    let status: TaskStatusPayload = deserialize_js(status, "task status");
    crate::get_status_progress(status.into())
}

#[benchmark_candidate(function, wasm_bindgen)]
pub fn is_status_complete(status: JsValue) -> bool {
    let status: TaskStatusPayload = deserialize_js(status, "task status");
    crate::is_status_complete(status.into())
}
