use std::sync::Mutex;

use boltffi::*;
use demo_bench_macros::benchmark_candidate;

use crate::records::blittable::DataPoint;

pub struct SharedCounter {
    value: Mutex<i32>,
}

impl Default for SharedCounter {
    fn default() -> Self {
        Self::new(0)
    }
}

#[export]
impl SharedCounter {
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

    pub fn increment(&self) -> i32 {
        let mut guard = self.value.lock().unwrap();
        *guard += 1;
        *guard
    }

    pub fn add(&self, amount: i32) -> i32 {
        let mut guard = self.value.lock().unwrap();
        *guard += amount;
        *guard
    }

    pub async fn async_get(&self) -> i32 {
        *self.value.lock().unwrap()
    }

    pub async fn async_add(&self, amount: i32) -> i32 {
        let mut guard = self.value.lock().unwrap();
        *guard += amount;
        *guard
    }
}

#[benchmark_candidate(object, uniffi)]
pub struct DataStore {
    items: Mutex<Vec<DataPoint>>,
}

impl Default for DataStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DataStore {
    #[cfg(not(feature = "uniffi"))]
    fn sample_items() -> Vec<DataPoint> {
        vec![
            DataPoint {
                x: 1.0,
                y: 2.0,
                timestamp: 100,
            },
            DataPoint {
                x: 3.0,
                y: 4.0,
                timestamp: 200,
            },
            DataPoint {
                x: 5.0,
                y: 6.0,
                timestamp: 300,
            },
        ]
    }

    #[cfg(not(feature = "uniffi"))]
    fn from_sample_data() -> Self {
        Self {
            items: Mutex::new(Self::sample_items()),
        }
    }

    #[cfg(not(feature = "uniffi"))]
    fn from_capacity(capacity: i32) -> Self {
        Self {
            items: Mutex::new(Vec::with_capacity(capacity.max(0) as usize)),
        }
    }

    #[cfg(not(feature = "uniffi"))]
    fn from_initial_point(x: f64, y: f64, timestamp: i64) -> Self {
        Self {
            items: Mutex::new(vec![DataPoint { x, y, timestamp }]),
        }
    }

    pub fn copy_into(&self, dst: &mut [DataPoint]) -> usize {
        let items = self.items.lock().unwrap();
        let len = items.len().min(dst.len());
        dst[..len].copy_from_slice(&items[..len]);
        len
    }
}

#[cfg(not(feature = "uniffi"))]
#[export]
impl DataStore {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
        }
    }

    pub fn with_sample_data() -> Self {
        Self::from_sample_data()
    }

    pub fn with_capacity(capacity: i32) -> Self {
        Self::from_capacity(capacity)
    }

    pub fn with_initial_point(x: f64, y: f64, timestamp: i64) -> Self {
        Self::from_initial_point(x, y, timestamp)
    }

    pub fn add(&self, point: DataPoint) {
        self.items.lock().unwrap().push(point);
    }

    pub fn add_parts(&self, x: f64, y: f64, timestamp: i64) {
        self.add(DataPoint { x, y, timestamp });
    }

    pub fn len(&self) -> usize {
        self.items.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.lock().unwrap().is_empty()
    }

    pub fn sum(&self) -> f64 {
        self.items
            .lock()
            .unwrap()
            .iter()
            .map(|point| point.x + point.y)
            .sum()
    }

    pub fn foreach(&self, mut callback: impl FnMut(DataPoint)) {
        self.items
            .lock()
            .unwrap()
            .iter()
            .copied()
            .for_each(|point| callback(point));
    }

    pub async fn async_sum(&self) -> Result<f64, String> {
        let items = self.items.lock().unwrap();
        if items.is_empty() {
            Err("no items to sum".to_string())
        } else {
            Ok(items.iter().map(|point| point.x + point.y).sum())
        }
    }

    pub async fn async_len(&self) -> Result<usize, String> {
        Ok(self.len())
    }
}

#[cfg(feature = "uniffi")]
#[benchmark_candidate(impl, uniffi, constructor = "new")]
impl DataStore {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
        }
    }

    pub fn add(&self, point: DataPoint) {
        self.items.lock().unwrap().push(point);
    }

    pub fn len(&self) -> u64 {
        self.items.lock().unwrap().len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.items.lock().unwrap().is_empty()
    }

    pub fn sum(&self) -> f64 {
        self.items
            .lock()
            .unwrap()
            .iter()
            .map(|point| point.x + point.y)
            .sum()
    }
}

#[benchmark_candidate(object, uniffi)]
pub struct Accumulator {
    value: Mutex<i64>,
}

impl Default for Accumulator {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
#[benchmark_candidate(impl, uniffi, constructor = "new")]
impl Accumulator {
    pub fn new() -> Self {
        Self {
            value: Mutex::new(0),
        }
    }

    pub fn add(&self, amount: i64) {
        *self.value.lock().unwrap() += amount;
    }

    pub fn get(&self) -> i64 {
        *self.value.lock().unwrap()
    }

    pub fn reset(&self) {
        *self.value.lock().unwrap() = 0;
    }
}
