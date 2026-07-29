use std::sync::Mutex;

use boltffi::*;

use crate::{FixtureMarkerOptions, FixtureMessageRecord, FixturePoint, FixtureStatus};

pub struct TestCounter {
    value: Mutex<i32>,
}

#[export]
impl TestCounter {
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
        let mut value = self.value.lock().unwrap();
        *value += amount;
        *value
    }

    pub async fn async_get(&self) -> i32 {
        self.get()
    }

    pub async fn async_add(&self, amount: i32) -> i32 {
        self.add(amount)
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

pub struct FixtureMarker {
    id: i32,
}

#[export(single_threaded)]
impl FixtureMarker {
    pub fn id(&self) -> i32 {
        self.id
    }
}

#[derive(Default)]
pub struct FixtureMap;

#[export(single_threaded)]
impl FixtureMap {
    pub fn new() -> Self {
        Self
    }

    pub fn add_marker(&self, options: FixtureMarkerOptions) -> FixtureMarker {
        FixtureMarker { id: options.id }
    }

    pub fn clone_handle(&self) -> Self {
        Self
    }

    pub fn maybe_marker(
        &self,
        options: FixtureMarkerOptions,
        should_create: bool,
    ) -> Option<FixtureMarker> {
        should_create.then_some(FixtureMarker { id: options.id })
    }

    pub fn default_marker(options: FixtureMarkerOptions) -> FixtureMarker {
        FixtureMarker { id: options.id }
    }
}

pub struct ClassTestFixture {
    state: Mutex<ClassTestFixtureState>,
}

struct ClassTestFixtureState {
    id: i32,
    name: String,
    point: FixturePoint,
    status: FixtureStatus,
    values: Vec<i32>,
    optional: Option<i32>,
}

#[export]
impl ClassTestFixture {
    fn new_state(
        id: i32,
        name: String,
        point: FixturePoint,
        status: FixtureStatus,
    ) -> Mutex<ClassTestFixtureState> {
        Mutex::new(ClassTestFixtureState {
            id,
            name,
            point,
            status,
            values: Vec::new(),
            optional: None,
        })
    }

    pub fn new_default() -> Self {
        Self {
            state: Self::new_state(
                0,
                String::new(),
                FixturePoint::default(),
                FixtureStatus::Pending,
            ),
        }
    }

    pub fn new_with_id(id: i32) -> Self {
        Self {
            state: Self::new_state(
                id,
                String::new(),
                FixturePoint::default(),
                FixtureStatus::Pending,
            ),
        }
    }

    pub fn new_with_name(name: String) -> Self {
        Self {
            state: Self::new_state(0, name, FixturePoint::default(), FixtureStatus::Pending),
        }
    }

    pub fn new_with_point(point: FixturePoint) -> Self {
        Self {
            state: Self::new_state(0, String::new(), point, FixtureStatus::Pending),
        }
    }

    pub fn new_with_status(status: FixtureStatus) -> Self {
        Self {
            state: Self::new_state(0, String::new(), FixturePoint::default(), status),
        }
    }

    pub fn new_full(id: i32, name: String, point: FixturePoint, status: FixtureStatus) -> Self {
        Self {
            state: Self::new_state(id, name, point, status),
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
        self.state.lock().unwrap().id
    }

    pub fn get_name(&self) -> String {
        self.state.lock().unwrap().name.clone()
    }

    pub fn get_point(&self) -> FixturePoint {
        self.state.lock().unwrap().point
    }

    pub fn get_status(&self) -> FixtureStatus {
        self.state.lock().unwrap().status
    }

    pub fn get_values(&self) -> Vec<i32> {
        self.state.lock().unwrap().values.clone()
    }

    pub fn get_optional(&self) -> Option<i32> {
        self.state.lock().unwrap().optional
    }

    pub fn set_id(&self, id: i32) {
        self.state.lock().unwrap().id = id;
    }

    pub fn set_name(&self, name: String) {
        self.state.lock().unwrap().name = name;
    }

    pub fn set_point(&self, point: FixturePoint) {
        self.state.lock().unwrap().point = point;
    }

    pub fn set_status(&self, status: FixtureStatus) {
        self.state.lock().unwrap().status = status;
    }

    pub fn set_values(&self, values: Vec<i32>) {
        self.state.lock().unwrap().values = values;
    }

    pub fn set_optional(&self, optional: Option<i32>) {
        self.state.lock().unwrap().optional = optional;
    }

    pub fn add_value(&self, value: i32) {
        self.state.lock().unwrap().values.push(value);
    }

    pub fn clear_values(&self) {
        self.state.lock().unwrap().values.clear();
    }

    pub fn values_count(&self) -> i32 {
        self.state.lock().unwrap().values.len() as i32
    }

    pub fn compute_sum(&self) -> i32 {
        self.state.lock().unwrap().values.iter().sum()
    }

    pub fn try_get_value(&self, index: i32) -> Result<i32, String> {
        let state = self.state.lock().unwrap();
        if index < 0 || index as usize >= state.values.len() {
            Err(format!("index {} out of bounds", index))
        } else {
            Ok(state.values[index as usize])
        }
    }

    pub fn find_value(&self, target: i32) -> Option<i32> {
        self.state
            .lock()
            .unwrap()
            .values
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
        self.get_id()
    }

    pub async fn async_get_name(&self) -> String {
        self.get_name()
    }

    pub async fn async_echo_message_record(
        &self,
        record: FixtureMessageRecord,
    ) -> FixtureMessageRecord {
        record
    }

    pub async fn async_set_id(&self, id: i32) {
        self.set_id(id);
    }

    pub async fn async_set_name(&self, name: String) {
        self.set_name(name);
    }

    pub async fn async_add_value(&self, value: i32) -> i32 {
        self.add_value(value);
        self.values_count()
    }

    pub async fn async_compute_sum(&self) -> i32 {
        self.compute_sum()
    }

    pub async fn async_try_get(&self, index: i32) -> Result<i32, String> {
        self.try_get_value(index)
    }

    pub async fn async_find(&self, target: i32) -> Option<i32> {
        self.find_value(target)
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
        self.state
            .lock()
            .unwrap()
            .values
            .iter()
            .copied()
            .filter(|&v| v.abs() <= threshold)
            .collect()
    }
}
