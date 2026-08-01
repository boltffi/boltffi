use std::sync::Mutex;

use boltffi::*;

/// A counter whose gated method holds the native call in flight inside a
/// caller-supplied callback, so tests can race `close()` against it.
pub struct GuardedCounter {
    value: Mutex<i32>,
}

#[export]
impl GuardedCounter {
    pub fn new(initial: i32) -> Self {
        Self {
            value: Mutex::new(initial),
        }
    }

    #[demo_bench_macros::demo_case(
        "classes.close_guard.guarded_counter.increment.should_reject_calls_after_close",
        justification = "Ensure a closed class handle rejects further method calls with the language-native closed-object error instead of reaching the freed native object.",
        directions = "Construct `classes::close_guard::GuardedCounter` through the generated binding, close it, and assert a subsequent `increment` call raises the language-native closed-object error.",
        exclude(
            swift,
            reason = ExclusionReason::ImplementationGap,
            details = "Swift releases class handles in deinit; there is no user-facing close() after which a call could be rejected."
        ),
        exclude(
            typescript,
            reason = ExclusionReason::ImplementationGap,
            details = "TypeScript releases class handles through FinalizationRegistry; there is no user-facing close() after which a call could be rejected."
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
        directions = "Call `classes::close_guard::GuardedCounter::increment_through_gate` on one thread, block inside the gate callback, close the handle from a second thread, release the gate, and assert the in-flight call returns the correct value while a subsequent call raises the language-native closed-object error.",
        exclude(
            swift,
            reason = ExclusionReason::ImplementationGap,
            details = "Swift releases class handles in deinit; there is no user-facing close() to race against an in-flight call."
        ),
        exclude(
            typescript,
            reason = ExclusionReason::ImplementationGap,
            details = "TypeScript releases class handles through FinalizationRegistry; there is no user-facing close() to race against an in-flight call."
        ),
        exclude(
            python,
            reason = ExclusionReason::ImplementationGap,
            details = "Python releases class handles in __del__; there is no user-facing close() to race against an in-flight call."
        )
    )]
    pub fn increment_through_gate(&self, gate: impl Fn(i32) -> i32) -> i32 {
        let delta = gate(*self.value.lock().unwrap());
        let mut guard = self.value.lock().unwrap();
        *guard += delta;
        *guard
    }
}
