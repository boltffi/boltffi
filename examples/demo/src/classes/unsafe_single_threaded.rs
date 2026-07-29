use std::cell::Cell;

use boltffi::*;

use crate::callbacks::sync_traits::ValueCallback;

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct MarkerOptions {
    pub id: u32,
    pub title: String,
}

pub struct Marker {
    id: u32,
    title: String,
}

#[export(single_threaded)]
impl Marker {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }
}

#[derive(Default)]
pub struct MapView {
    markers_created: Cell<u32>,
}

#[export(single_threaded)]
impl MapView {
    pub fn new() -> Self {
        Self {
            markers_created: Cell::new(0),
        }
    }

    #[demo_bench_macros::demo_case(
        "classes.unsafe_single_threaded.map_view.add_marker.should_return_single_threaded_marker_handle",
        justification = "Ensure a single-threaded class method returns another single-threaded class as an owned object handle.",
        directions = "Call `classes::unsafe_single_threaded::MapView::add_marker` through the generated binding and assert the returned Marker handle exposes the MarkerOptions fields."
    )]
    pub fn add_marker(&self, options: MarkerOptions) -> Marker {
        self.markers_created.set(self.markers_created.get() + 1);
        Marker {
            id: options.id,
            title: options.title,
        }
    }

    pub fn marker_count(&self) -> u32 {
        self.markers_created.get()
    }
}

pub struct StateHolder {
    label: String,
    value: i32,
    items: Vec<String>,
}

// skips all synchronization (Arc<Mutex<T>>) and uses UnsafeCell internally,
// so you get direct &mut self. calling from multiple threads is UB.
// useful when you know only one thread touches this, e.g. main-thread-only
// UI objects on iOS/Android where you don't want to pay for locks you'll
// never need. proceed with caution.
#[export(single_threaded)]
impl StateHolder {
    pub fn new(label: String) -> Self {
        Self {
            label,
            value: 0,
            items: Vec::new(),
        }
    }

    pub fn get_label(&self) -> String {
        self.label.clone()
    }

    pub fn get_value(&self) -> i32 {
        self.value
    }

    pub fn set_value(&mut self, value: i32) {
        self.value = value;
    }

    pub fn increment(&mut self) -> i32 {
        self.value += 1;
        self.value
    }

    pub fn add_item(&mut self, item: String) {
        self.items.push(item);
    }

    pub fn remove_last(&mut self) -> Option<String> {
        self.items.pop()
    }

    pub fn get_items(&self) -> Vec<String> {
        self.items.clone()
    }

    pub fn item_count(&self) -> u32 {
        self.items.len() as u32
    }

    pub fn clear(&mut self) {
        self.value = 0;
        self.items.clear();
    }

    pub fn transform_value(&mut self, f: impl Fn(i32) -> i32) -> i32 {
        self.value = f(self.value);
        self.value
    }

    pub fn apply_value_callback(&mut self, callback: impl ValueCallback) -> i32 {
        self.value = callback.on_value(self.value);
        self.value
    }

}

#[derive(Default)]
pub struct CounterSingleThreaded {
    value: i32,
}

#[export(single_threaded)]
impl CounterSingleThreaded {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn set(&mut self, value: i32) {
        self.value = value;
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn get(&self) -> i32 {
        self.value
    }
}

#[derive(Default)]
pub struct AccumulatorSingleThreaded {
    value: i64,
}

#[export(single_threaded)]
impl AccumulatorSingleThreaded {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn add(&mut self, amount: i64) {
        self.value += amount;
    }

    pub fn get(&self) -> i64 {
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0;
    }
}
