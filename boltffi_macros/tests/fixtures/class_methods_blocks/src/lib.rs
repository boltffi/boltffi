use std::sync::{Arc, Mutex};

use boltffi::{EventSubscription, StreamProducer, data, export};

boltffi::scaffolding!();

pub mod extras;
#[cfg(feature = "data_impl_on_class")]
mod data_impl_on_class;
#[cfg(feature = "methods_on_data")]
mod methods_on_data;
#[cfg(feature = "undeclared")]
mod undeclared;

#[data]
#[derive(Clone, Copy)]
pub struct Step {
    pub by: i32,
}

pub struct Counter {
    value: Mutex<i32>,
    changes: StreamProducer<i32>,
}

#[export(methods)]
impl Counter {
    pub const LIMIT: i32 = 100;

    pub fn starting_at(value: i32) -> Self {
        Self {
            value: Mutex::new(value),
            changes: StreamProducer::new(16),
        }
    }

    pub fn step(&self, step: Step) -> i32 {
        *self.value.lock().expect("counter lock") += step.by;
        self.get()
    }

    #[boltffi::ffi_stream(item = i32)]
    pub fn changes(&self) -> Arc<EventSubscription<i32>> {
        self.changes.subscribe()
    }
}

#[export]
impl Counter {
    pub const START: i32 = 0;

    pub fn new() -> Self {
        Self::starting_at(Self::START)
    }

    pub fn get(&self) -> i32 {
        *self.value.lock().expect("counter lock")
    }

    pub fn merged(&self, other: &Counter) -> i32 {
        self.get() + other.get()
    }
}

pub struct Cursor {
    position: u32,
}

#[export(single_threaded)]
impl Cursor {
    pub fn new() -> Self {
        Self { position: 0 }
    }
}

#[export(methods)]
impl Cursor {
    pub fn advance(&mut self) -> u32 {
        self.position += 1;
        self.position
    }
}

pub fn every_block_exports_its_members() -> [*const (); 10] {
    [
        boltffi_init_class_class_methods_blocks_counter_new as *const (),
        boltffi_init_class_class_methods_blocks_counter_starting_at as *const (),
        boltffi_method_class_class_methods_blocks_counter_get as *const (),
        boltffi_method_class_class_methods_blocks_counter_step as *const (),
        extras::by_path::boltffi_method_class_class_methods_blocks_counter_settled as *const (),
        extras::by_path::boltffi_method_class_class_methods_blocks_counter_twin as *const (),
        extras::by_import::boltffi_method_class_class_methods_blocks_counter_reset as *const (),
        boltffi_stream_class_methods_blocks_counter_changes_subscribe as *const (),
        extras::by_import::boltffi_stream_class_methods_blocks_counter_resets_subscribe
            as *const (),
        boltffi_method_class_class_methods_blocks_cursor_advance as *const (),
    ]
}
