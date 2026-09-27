use boltffi::export;

use crate::classes::ownership::{MessageDrops, OwnedMessage};
use crate::results::error_enums::MathError;

#[export]
pub trait MessageReceiver {
    fn attach(&self, r#handle: OwnedMessage, callback: u32);
    fn optional(&self, handle: Option<OwnedMessage>);
}

#[export]
pub trait FallibleMessageReceiver {
    fn pair(
        &self,
        first: OwnedMessage,
        label: String,
        second: Option<OwnedMessage>,
    ) -> Result<i32, MathError>;
}

#[demo_bench_macros::demo_case(
    "case:callbacks.class_handles.should_retain_after_return",
    justification = "A class passed to a callback belongs to the receiving wrapper.",
    directions = "Retain the message in the callback, use it after delivery returns, then release it and verify exactly one Rust drop."
)]
#[export]
pub fn deliver_message(receiver: Box<dyn MessageReceiver>, drops: &MessageDrops) {
    receiver.attach(OwnedMessage::new("from Rust".to_owned(), drops), 42);
}

#[demo_bench_macros::demo_case(
    "case:callbacks.class_handles.should_deliver_multiple_and_optional",
    justification = "Each class argument transfers its own ownership, including an optional class.",
    directions = "Deliver the pair with and without its second message, retain the received wrappers, and verify their lengths and drop counts."
)]
#[demo_bench_macros::demo_case(
    "case:callbacks.class_handles.should_retain_after_error",
    justification = "A callback error does not revoke ownership of messages already delivered.",
    directions = "Store both messages before returning an error, use them after the error reaches Rust, and release each exactly once."
)]
#[export]
pub fn deliver_message_pair(
    receiver: Box<dyn FallibleMessageReceiver>,
    drops: &MessageDrops,
    present: bool,
) -> Result<i32, MathError> {
    receiver.pair(
        OwnedMessage::new("first".to_owned(), drops),
        "pair".to_owned(),
        present.then(|| OwnedMessage::new("second".to_owned(), drops)),
    )
}

struct MeasuringReceiver;

impl MessageReceiver for MeasuringReceiver {
    fn attach(&self, r#handle: OwnedMessage, callback: u32) {
        assert_eq!(callback, 42);
        drop(r#handle);
    }

    fn optional(&self, handle: Option<OwnedMessage>) {
        drop(handle);
    }
}

#[demo_bench_macros::demo_case(
    "case:callbacks.class_handles.should_consume_in_rust_callback",
    justification = "Calling a Rust callback proxy moves owned class arguments back into Rust.",
    directions = "Pass owned messages to the returned callback and verify they are consumed exactly once, including an optional message.",
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python does not expose returned Rust callback proxies"
    )
)]
#[export]
pub fn make_message_receiver() -> Box<dyn MessageReceiver> {
    Box::new(MeasuringReceiver)
}
