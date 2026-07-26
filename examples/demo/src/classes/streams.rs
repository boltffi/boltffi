use std::sync::Arc;

use boltffi::*;

use crate::records::blittable::Point;

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct StreamMessage {
    pub text: String,
    pub values: Vec<i32>,
}

/// Publishes events to subscribers. Clients call `subscribe_values`
/// or `subscribe_points` to get a stream, then poll for new items.
pub struct EventBus {
    int_producer: StreamProducer<i32>,
    point_producer: StreamProducer<Point>,
    message_producer: StreamProducer<StreamMessage>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[export]
impl EventBus {
    pub fn new() -> Self {
        Self {
            int_producer: StreamProducer::new(256),
            point_producer: StreamProducer::new(64),
            message_producer: StreamProducer::new(32),
        }
    }

    pub fn emit_value(&self, value: i32) {
        self.int_producer.push(value);
    }

    pub fn emit_point(&self, point: Point) {
        self.point_producer.push(point);
    }

    pub fn emit_message(&self, message: StreamMessage) {
        self.message_producer.push(message);
    }

    pub fn emit_batch(&self, values: Vec<i32>) -> u32 {
        values
            .iter()
            .inspect(|&&value| {
                self.int_producer.push(value);
            })
            .count() as u32
    }

    /// Subscribe to the integer event stream.
    #[ffi_stream(item = i32)]
    pub fn subscribe_values(&self) -> Arc<EventSubscription<i32>> {
        self.int_producer.subscribe()
    }

    #[ffi_stream(item = Point)]
    pub fn subscribe_points(&self) -> Arc<EventSubscription<Point>> {
        self.point_producer.subscribe()
    }

    #[demo_bench_macros::demo_case(
        "classes.streams.event_bus.subscribe_messages.should_deliver_encoded_record_items",
        justification = "Ensure stream items that use wire encoding, including String and Vec fields, are delivered without treating heap-backed data as direct memory.",
        directions = "Subscribe to `classes::streams::EventBus::subscribe_messages`, emit two StreamMessage values through `emit_message`, and assert the generated binding receives both messages with their text and values intact.",
        exercises = [
            "classes::streams::StreamMessage",
            "classes::streams::EventBus::emit_message",
            "classes::streams::EventBus::subscribe_messages",
        ]
    )]
    #[ffi_stream(item = StreamMessage)]
    pub fn subscribe_messages(&self) -> Arc<EventSubscription<StreamMessage>> {
        self.message_producer.subscribe()
    }

    #[ffi_stream(item = i32, mode = "batch")]
    pub fn subscribe_values_batch(&self) -> Arc<EventSubscription<i32>> {
        self.int_producer.subscribe()
    }

    #[ffi_stream(item = i32, mode = "callback")]
    pub fn subscribe_values_callback(&self) -> Arc<EventSubscription<i32>> {
        self.int_producer.subscribe()
    }
}
