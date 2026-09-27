use std::sync::{Arc, Mutex};

use boltffi::*;

/// A counter whose gated method holds the native call in flight inside a
/// caller-supplied callback, so tests can race `close()` against it.
pub struct GuardedCounter {
    value: Mutex<i32>,
    lifetime: Arc<()>,
}

#[export]
impl GuardedCounter {
    pub fn new(initial: i32) -> Self {
        Self {
            value: Mutex::new(initial),
            lifetime: Arc::new(()),
        }
    }

    #[demo_bench_macros::demo_case(
        "classes.close_guard.guarded_counter.increment.should_reject_calls_after_close",
        justification = "Ensure a closed class handle rejects further method calls with the language-native closed-object error instead of reaching the freed native object.",
        directions = "Construct `classes::close_guard::GuardedCounter` through the generated binding, close it, and assert a subsequent `increment` call raises the language-native closed-object error.",
        exclude(
            c,
            reason = ExclusionReason::ImplementationGap,
            details = "C callers own raw class handles and release them explicitly; there is no binding-level close guard."
        ),
        exclude(
            swift,
            reason = ExclusionReason::ImplementationGap,
            details = "Swift releases class handles in deinit; there is no user-facing close() after which a call could be rejected."
        ),
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python releases class handles in __del__; there is no user-facing close() after which a call could be rejected."
        )
    )]
    pub fn increment(&self) -> i32 {
        let mut guard = self.value.lock().unwrap();
        *guard += 1;
        *guard
    }

    #[demo_bench_macros::demo_case(
        "classes.close_guard.guarded_counter.increment_through_gate.should_complete_in_flight_call_when_closed",
        justification = "Ensure close() during an in-flight method call defers freeing the native object until the call completes, so the call finishes against live memory and only later calls fail (issue #664).",
        directions = "Call `classes::close_guard::GuardedCounter::increment_through_gate`, close the handle while inside the gate callback (from a second thread or reentrantly on single-isolate targets), then return from the gate and assert the in-flight call returns the correct value while a subsequent call raises the language-native closed-object error.",
        exclude(
            c,
            reason = ExclusionReason::ImplementationGap,
            details = "C callers own raw class handles and release them explicitly; there is no binding-level close guard."
        ),
        exclude(
            swift,
            reason = ExclusionReason::ImplementationGap,
            details = "Swift releases class handles in deinit; there is no user-facing close() to race against an in-flight call."
        ),
        exclude(
            typescript,
            reason = ExclusionReason::ImplementationGap,
            details = "JavaScript is single-threaded, so dispose() cannot race an in-flight call from another thread; the deferred-free scenario cannot be expressed."
        ),
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python releases class handles in __del__; there is no user-facing close() to race against an in-flight call."
        )
    )]
    pub fn increment_through_gate(&self, gate: impl Fn(i32) -> i32) -> i32 {
        // A weak token detects premature destruction even when freed memory
        // still happens to contain the expected counter value.
        let lifetime = Arc::downgrade(&self.lifetime);
        let observed = *self.value.lock().unwrap();
        let delta = gate(observed);
        assert!(
            lifetime.upgrade().is_some(),
            "GuardedCounter was freed during an in-flight call"
        );
        let mut guard = self.value.lock().unwrap();
        *guard += delta;
        *guard
    }
}

#[demo_bench_macros::demo_case(
    "classes.close_guard.consume_guarded_counter.should_reject_transfer_during_in_flight_call",
    justification = "Ensure passing a class by value while one of its methods is still in flight fails with the language-native error and closes the object, instead of handing Rust ownership of memory the in-flight call still borrows.",
    directions = "Call `classes::close_guard::GuardedCounter::increment_through_gate` and, inside the gate, pass the same counter to `classes::close_guard::consume_guarded_counter`. Assert that call raises the language-native closed-object error naming the in-flight call, that the gated call still returns the correct value, and that a subsequent `increment` raises the closed-object error.",
    exclude(
        c,
        reason = ExclusionReason::ImplementationGap,
        details = "C callers own raw class handles and release them explicitly; there is no binding-level close guard."
    ),
    exclude(
        swift,
        reason = ExclusionReason::ImplementationGap,
        details = "Swift's by-value handle take does not track in-flight calls yet."
    ),
    exclude(
        typescript,
        reason = ExclusionReason::ImplementationGap,
        details = "The TypeScript by-value handle take does not track in-flight calls yet."
    ),
    exclude(
        python,
        reason = ExclusionReason::ImplementationGap,
        details = "Python's by-value handle take does not track in-flight calls yet."
    )
)]
#[export]
pub fn consume_guarded_counter(counter: GuardedCounter) -> i32 {
    *counter.value.lock().unwrap()
}
