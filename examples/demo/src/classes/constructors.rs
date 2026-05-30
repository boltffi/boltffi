use std::sync::Mutex;

use boltffi::*;

/// A bounded collection of named items.
///
/// Shows multiple constructor patterns: a default constructor,
/// a parameterized one, and a fallible one that returns Result.
pub struct Inventory {
    items: Mutex<Vec<String>>,
    capacity: u32,
}

#[export]
impl Inventory {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            capacity: 100,
        }
    }

    pub fn with_capacity(capacity: u32) -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            capacity,
        }
    }

    /// Creates an inventory, or fails if capacity is zero.
    #[demo_bench_macros::demo_case(
        "classes.constructors.inventory.try_new.should_return_inventory_for_positive_capacity",
        justification = "Ensure a fallible class constructor returns an owned class handle through Result Ok.",
        directions = "Call `classes::constructors::Inventory::try_new` through the generated binding with a positive capacity and assert it returns a usable Inventory instance.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer does not currently emit class/object bindings. Include this case when Python class bindings are implemented."
        )
    )]
    #[demo_bench_macros::demo_case(
        "classes.constructors.inventory.try_new.should_reject_zero_capacity",
        justification = "Ensure a fallible class constructor reports Result Err through the target language failure path.",
        directions = "Call `classes::constructors::Inventory::try_new` through the generated binding with zero capacity and assert the target language observes the constructor failure.",
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python is experimental; its lowerer does not currently emit class/object bindings. Include this case when Python class bindings are implemented."
        )
    )]
    pub fn try_new(capacity: u32) -> Result<Self, String> {
        if capacity == 0 {
            Err("capacity must be greater than zero".to_string())
        } else {
            Ok(Self {
                items: Mutex::new(Vec::new()),
                capacity,
            })
        }
    }

    pub fn count(&self) -> u32 {
        self.items.lock().unwrap().len() as u32
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn add(&self, item: String) -> bool {
        let mut items = self.items.lock().unwrap();
        if (items.len() as u32) < self.capacity {
            items.push(item);
            true
        } else {
            false
        }
    }

    pub fn remove(&self, index: u32) -> Option<String> {
        let mut items = self.items.lock().unwrap();
        if (index as usize) < items.len() {
            Some(items.remove(index as usize))
        } else {
            None
        }
    }

    pub fn get_all(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }
}
