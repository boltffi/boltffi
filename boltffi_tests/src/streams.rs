use std::sync::Arc;

use boltffi::*;

use crate::FixturePoint;

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

pub struct LabelStream {
    producer: StreamProducer<String>,
}

impl Default for LabelStream {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl LabelStream {
    pub fn new() -> Self {
        Self {
            producer: StreamProducer::new(32),
        }
    }

    pub fn emit(&self, label: String) {
        self.producer.push(label);
    }

    #[ffi_stream(item = String)]
    pub fn subscribe(&self) -> Arc<EventSubscription<String>> {
        self.producer.subscribe()
    }
}

pub struct JobStream {
    subscription: Arc<EventSubscription<i32, String>>,
}

impl Default for JobStream {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl JobStream {
    pub fn new() -> Self {
        Self {
            subscription: Arc::new(EventSubscription::fallible(8)),
        }
    }

    pub fn emit(&self, value: i32) -> bool {
        self.subscription.push_event(value)
    }

    pub fn fail(&self, reason: String) -> bool {
        self.subscription.fail(reason)
    }

    #[ffi_stream(item = i32, error = String)]
    pub fn subscribe(&self) -> Arc<EventSubscription<i32, String>> {
        Arc::clone(&self.subscription)
    }
}
