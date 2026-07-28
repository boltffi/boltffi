use std::{future::Future, marker::PhantomData, rc::Rc, sync::Mutex};

use boltffi::*;

use crate::{FixtureMarkerOptions, FixtureMessageRecord, FixturePoint, FixtureStatus};

pub struct TestCounter {
    value: i32,
    _single_threaded_marker: PhantomData<Rc<()>>,
}

#[export(single_threaded)]
impl TestCounter {
    pub fn new(initial: i32) -> Self {
        Self {
            value: initial,
            _single_threaded_marker: PhantomData,
        }
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

    pub fn async_get(&self) -> impl Future<Output = i32> + Send + 'static {
        let value = self.value;
        async move { value }
    }

    pub fn async_add(&mut self, amount: i32) -> impl Future<Output = i32> + Send + 'static {
        self.value += amount;
        let value = self.value;
        async move { value }
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

    pub fn async_get_id(&self) -> impl Future<Output = i32> + Send + 'static {
        let id = self.id;
        async move { id }
    }

    pub fn async_get_name(&self) -> impl Future<Output = String> + Send + 'static {
        let name = self.name.clone();
        async move { name }
    }

    pub fn async_echo_message_record(
        &self,
        record: FixtureMessageRecord,
    ) -> impl Future<Output = FixtureMessageRecord> + Send + 'static {
        async move { record }
    }

    pub fn async_set_id(&mut self, id: i32) -> impl Future<Output = ()> + Send + 'static {
        self.id = id;
        async {}
    }

    pub fn async_set_name(&mut self, name: String) -> impl Future<Output = ()> + Send + 'static {
        self.name = name;
        async {}
    }

    pub fn async_add_value(&mut self, value: i32) -> impl Future<Output = i32> + Send + 'static {
        self.values.push(value);
        let count = self.values.len() as i32;
        async move { count }
    }

    pub fn async_compute_sum(&self) -> impl Future<Output = i32> + Send + 'static {
        let sum = self.values.iter().sum();
        async move { sum }
    }

    pub fn async_try_get(
        &self,
        index: i32,
    ) -> impl Future<Output = Result<i32, String>> + Send + 'static {
        let result = if index < 0 || index as usize >= self.values.len() {
            Err(format!("index {} out of bounds", index))
        } else {
            Ok(self.values[index as usize])
        };
        async move { result }
    }

    pub fn async_find(&self, target: i32) -> impl Future<Output = Option<i32>> + Send + 'static {
        let index = self
            .values
            .iter()
            .position(|&v| v == target)
            .map(|index| index as i32);
        async move { index }
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
