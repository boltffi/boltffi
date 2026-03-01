use crate::ringbuffer::SpscRingBuffer;
use std::{
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, Ordering},
    },
    time::Duration,
};

#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPollResult {
    Ready = 0,
    Closed = 1,
}

pub type StreamContinuationCallback = extern "C" fn(callback_data: u64, StreamPollResult);

/// States for the lock-free continuation scheduler. Transitions use atomic CAS.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ContinuationState {
    /// No continuation is parked and no wake has been buffered.
    /// Initial state after construction or after a successful callback invocation.
    Empty = 0,
    /// A wake arrived before a continuation was stored. The next `store()` call
    /// will fire the callback immediately and transition back to `Empty`.
    Waked = 1,
    /// A continuation is parked and waiting. A subsequent `wake()` or `cancel()`
    /// will invoke the stored callback and transition out.
    Stored = 2,
    /// The stream has been torn down. Terminal state, no further transitions are valid.
    /// Any future `store()` call receives `Closed` immediately.
    Cancelled = 3,
}

impl ContinuationState {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Empty,
            1 => Self::Waked,
            2 => Self::Stored,
            3 => Self::Cancelled,
            _ => Self::Empty,
        }
    }
}

/// A lock-free scheduler that coordinates handoff between a stream producer and a
/// single parked continuation.
///
/// # Overview
///
/// The scheduler mediates the race between two sides of a stream: the consumer that
/// parks a continuation via [`store`](Self::store_continuation), and the producer that
/// signals data availability via [`wake`](Self::wake). Both sides may arrive in any
/// order; the scheduler resolves the race without locks using atomic compare-and-swap
/// on a four-state machine.
///
/// # States
///
/// | State | Meaning |
/// |-------------|---------|
/// | `Empty` | Idle. No continuation parked, no pending wake. |
/// | `Stored` | A continuation is parked and waiting for data. |
/// | `Waked` | Data arrived before a continuation was parked. |
/// | `Cancelled` | Terminal. The stream has been torn down. |
///
/// # State Diagram
///
/// ```text
///              store()          wake()
///   Empty ─────────────► Stored ─────────────► Empty
///     │                    │                     (invokes callback)
///     │ wake()             │ cancel()
///     ▼                    ▼
///   Waked                Cancelled
///     │                    (invokes callback
///     │ store()             with Closed)
///     ▼
///   Empty
///   (invokes callback
///    immediately)
/// ```
///
/// # Transitions
///
/// - **`store()` in `Empty`** → `Stored`. The continuation is parked.
/// - **`store()` in `Waked`** → `Empty`. The wake was buffered, so the continuation
///   fires immediately with `Ready`.
/// - **`store()` in `Cancelled`** → stays `Cancelled`. The continuation fires
///   immediately with `Closed`.
/// - **`wake()` in `Stored`** → `Empty`. The parked continuation fires with `Ready`.
/// - **`wake()` in `Empty`** → `Waked`. The wake is buffered for the next `store()`.
/// - **`cancel()` in `Stored`** → `Cancelled`. The parked continuation fires with `Closed`.
/// - **`cancel()` in `Empty` or `Waked`** → `Cancelled`. No callback to invoke.
///
/// # Thread Safety
///
/// All transitions use `compare_exchange` with acquire-release ordering. The callback
/// pointer and data are stored with release semantics before the state transition, and
/// loaded with acquire semantics after, ensuring the callback is fully visible to
/// whichever thread wins the CAS.
struct StreamContinuationScheduler {
    state: AtomicU8,
    callback_data: AtomicU64,
    callback_ptr: AtomicPtr<()>,
}

impl StreamContinuationScheduler {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(ContinuationState::Empty as u8),
            callback_data: AtomicU64::new(0),
            callback_ptr: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    fn current_state(&self) -> ContinuationState {
        ContinuationState::from_raw(self.state.load(Ordering::Acquire))
    }

    fn try_transition(&self, from: ContinuationState, to: ContinuationState) -> bool {
        self.state.compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire).is_ok()
    }

    fn store_continuation(&self, callback: StreamContinuationCallback, callback_data: u64) {
        loop {
            match self.current_state() {
                ContinuationState::Empty => {
                    self.callback_data.store(callback_data, Ordering::Release);
                    self.callback_ptr.store(callback as *mut (), Ordering::Release);
                    if self.try_transition(ContinuationState::Empty, ContinuationState::Stored) {
                        return;
                    }
                }
                ContinuationState::Waked => {
                    if self.try_transition(ContinuationState::Waked, ContinuationState::Empty) {
                        callback(callback_data, StreamPollResult::Ready);
                        return;
                    }
                }
                ContinuationState::Stored => {
                    self.invoke_stored(StreamPollResult::Ready);
                    self.callback_data.store(callback_data, Ordering::Release);
                    self.callback_ptr.store(callback as *mut (), Ordering::Release);
                    return;
                }
                ContinuationState::Cancelled => {
                    callback(callback_data, StreamPollResult::Closed);
                    return;
                }
            }
        }
    }

    fn wake(&self) {
        loop {
            match self.current_state() {
                ContinuationState::Stored => {
                    if self.try_transition(ContinuationState::Stored, ContinuationState::Empty) {
                        self.invoke_stored(StreamPollResult::Ready);
                        return;
                    }
                }
                ContinuationState::Empty => {
                    if self.try_transition(ContinuationState::Empty, ContinuationState::Waked) {
                        return;
                    }
                }
                ContinuationState::Waked | ContinuationState::Cancelled => return,
            }
        }
    }

    fn cancel(&self) {
        loop {
            match self.current_state() {
                ContinuationState::Stored => {
                    if self.try_transition(ContinuationState::Stored, ContinuationState::Cancelled) {
                        self.invoke_stored(StreamPollResult::Closed);
                        return;
                    }
                }
                ContinuationState::Empty | ContinuationState::Waked => {
                    if self.try_transition(self.current_state(), ContinuationState::Cancelled) {
                        return;
                    }
                }
                ContinuationState::Cancelled => return,
            }
        }
    }

    fn invoke_stored(&self, result: StreamPollResult) {
        let callback_ptr = self.callback_ptr.load(Ordering::Acquire);
        let callback_data = self.callback_data.load(Ordering::Acquire);
        if !callback_ptr.is_null() {
            let callback: StreamContinuationCallback = unsafe { std::mem::transmute(callback_ptr) };
            callback(callback_data, result);
        }
    }
}

pub struct EventSubscription<T: Send + 'static> {
    ring_buffer: SpscRingBuffer<T>,
    is_active: AtomicBool,
    notification_mutex: Mutex<()>,
    notification_condvar: Condvar,
    continuation_scheduler: StreamContinuationScheduler,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    EventsAvailable = 1,
    Timeout = 0,
    Unsubscribed = -1,
}

impl<T: Send + 'static> EventSubscription<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            ring_buffer: SpscRingBuffer::new(capacity),
            is_active: AtomicBool::new(true),
            notification_mutex: Mutex::new(()),
            notification_condvar: Condvar::new(),
            continuation_scheduler: StreamContinuationScheduler::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Acquire)
    }

    pub fn push_event(&self, event: T) -> bool {
        if !self.is_active() {
            return false;
        }

        let push_succeeded = self.ring_buffer.push(event).is_ok();

        if push_succeeded {
            self.notification_condvar.notify_one();
            self.continuation_scheduler.wake();
        }

        push_succeeded
    }

    pub fn pop_event(&self) -> Option<T> {
        self.ring_buffer.pop()
    }

    pub fn pop_batch_into(&self, output_buffer: &mut [std::mem::MaybeUninit<T>]) -> usize {
        self.ring_buffer.pop_batch_into(output_buffer)
    }

    pub fn wait_for_events(&self, timeout_milliseconds: u32) -> WaitResult {
        if !self.is_active() {
            return WaitResult::Unsubscribed;
        }

        if self.ring_buffer.available_count() > 0 {
            return WaitResult::EventsAvailable;
        }

        let notification_guard = self.notification_mutex.lock().unwrap();
        let timeout_duration = Duration::from_millis(timeout_milliseconds as u64);

        let wait_result = self
            .notification_condvar
            .wait_timeout_while(notification_guard, timeout_duration, |_| self.is_active() && self.ring_buffer.is_empty());

        if !self.is_active() {
            return WaitResult::Unsubscribed;
        }

        match wait_result {
            Ok((_, timeout_result)) if timeout_result.timed_out() => WaitResult::Timeout,
            _ => {
                if self.ring_buffer.available_count() > 0 {
                    WaitResult::EventsAvailable
                } else {
                    WaitResult::Timeout
                }
            }
        }
    }

    pub fn poll(&self, callback_data: u64, callback: StreamContinuationCallback) {
        if !self.is_active() {
            callback(callback_data, StreamPollResult::Closed);
            return;
        }

        if self.ring_buffer.available_count() > 0 {
            callback(callback_data, StreamPollResult::Ready);
            return;
        }

        self.continuation_scheduler.store_continuation(callback, callback_data);
    }

    pub fn unsubscribe(&self) {
        self.is_active.store(false, Ordering::Release);
        self.notification_condvar.notify_all();
        self.continuation_scheduler.cancel();
    }

    pub fn available_count(&self) -> usize {
        self.ring_buffer.available_count()
    }
}

impl<T: Send + 'static> Drop for EventSubscription<T> {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

pub type SubscriptionHandle = *mut core::ffi::c_void;

pub fn subscription_new<T: Send + 'static>(capacity: usize) -> SubscriptionHandle {
    let subscription = Box::new(EventSubscription::<T>::new(capacity));
    Box::into_raw(subscription) as SubscriptionHandle
}

pub unsafe fn subscription_push<T: Send + 'static>(handle: SubscriptionHandle, event: T) -> bool {
    if handle.is_null() {
        return false;
    }
    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.push_event(event)
}

pub unsafe fn subscription_pop_batch<T: Send + Copy + 'static>(handle: SubscriptionHandle, output_ptr: *mut T, output_capacity: usize) -> usize {
    if handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
        return 0;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    let output_slice = unsafe { std::slice::from_raw_parts_mut(output_ptr as *mut std::mem::MaybeUninit<T>, output_capacity) };

    subscription.pop_batch_into(output_slice)
}

pub unsafe fn subscription_wait<T: Send + 'static>(handle: SubscriptionHandle, timeout_milliseconds: u32) -> i32 {
    if handle.is_null() {
        return WaitResult::Unsubscribed as i32;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.wait_for_events(timeout_milliseconds) as i32
}

pub unsafe fn subscription_poll<T: Send + 'static>(handle: SubscriptionHandle, callback_data: u64, callback: StreamContinuationCallback) {
    if handle.is_null() {
        callback(callback_data, StreamPollResult::Closed);
        return;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.poll(callback_data, callback);
}

pub unsafe fn subscription_unsubscribe<T: Send + 'static>(handle: SubscriptionHandle) {
    if handle.is_null() {
        return;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.unsubscribe();
}

pub unsafe fn subscription_free<T: Send + 'static>(handle: SubscriptionHandle) {
    if handle.is_null() {
        return;
    }

    let subscription = unsafe { Box::from_raw(handle as *mut EventSubscription<T>) };
    drop(subscription);
}

struct SubscriberSlot<T: Send + 'static> {
    weak_ptr: AtomicPtr<()>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> SubscriberSlot<T> {
    const fn empty() -> Self {
        Self {
            weak_ptr: AtomicPtr::new(std::ptr::null_mut()),
            _marker: std::marker::PhantomData,
        }
    }

    fn try_claim(&self, subscription: &Arc<EventSubscription<T>>) -> bool {
        let weak = Arc::downgrade(subscription);
        let raw_ptr = Weak::into_raw(weak) as *mut ();

        match self.weak_ptr.compare_exchange(std::ptr::null_mut(), raw_ptr, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => true,
            Err(_) => {
                unsafe { Weak::from_raw(raw_ptr as *const EventSubscription<T>) };
                false
            }
        }
    }

    fn upgrade(&self) -> Option<Arc<EventSubscription<T>>> {
        let ptr = self.weak_ptr.load(Ordering::Acquire);
        if ptr.is_null() {
            return None;
        }

        let weak = unsafe { Weak::from_raw(ptr as *const EventSubscription<T>) };
        let strong = weak.upgrade();
        std::mem::forget(weak);
        strong
    }

    fn clear_if_dead(&self) {
        let ptr = self.weak_ptr.load(Ordering::Acquire);
        if ptr.is_null() {
            return;
        }

        let weak = unsafe { Weak::from_raw(ptr as *const EventSubscription<T>) };
        let is_dead = weak.strong_count() == 0;
        std::mem::forget(weak);

        let successfully_cleared = is_dead && self.weak_ptr.compare_exchange(ptr, std::ptr::null_mut(), Ordering::AcqRel, Ordering::Acquire).is_ok();

        if successfully_cleared {
            unsafe { Weak::from_raw(ptr as *const EventSubscription<T>) };
        }
    }

    fn is_alive(&self) -> bool {
        self.upgrade().map(|sub| sub.is_active()).unwrap_or(false)
    }
}

impl<T: Send + 'static> Drop for SubscriberSlot<T> {
    fn drop(&mut self) {
        let ptr = *self.weak_ptr.get_mut();
        if !ptr.is_null() {
            unsafe { Weak::from_raw(ptr as *const EventSubscription<T>) };
        }
    }
}

pub struct StreamProducer<T: Send + Copy + 'static, const MAX_SUBSCRIBERS: usize = 32> {
    subscriber_slots: [SubscriberSlot<T>; MAX_SUBSCRIBERS],
    default_capacity: usize,
}

impl<T: Send + Copy + 'static, const MAX_SUBSCRIBERS: usize> StreamProducer<T, MAX_SUBSCRIBERS> {
    pub fn new(default_capacity: usize) -> Self {
        Self {
            subscriber_slots: core::array::from_fn(|_| SubscriberSlot::empty()),
            default_capacity,
        }
    }

    pub fn subscribe(&self) -> Arc<EventSubscription<T>> {
        self.subscribe_with_capacity(self.default_capacity)
    }

    pub fn subscribe_with_capacity(&self, capacity: usize) -> Arc<EventSubscription<T>> {
        let subscription = Arc::new(EventSubscription::new(capacity));

        self.subscriber_slots.iter().for_each(|slot| slot.clear_if_dead());

        let slot_claimed = self.subscriber_slots.iter().any(|slot| slot.try_claim(&subscription));

        if !slot_claimed {
            eprintln!("StreamProducer: all {} subscriber slots full", MAX_SUBSCRIBERS);
        }

        subscription
    }

    pub fn push(&self, event: T) {
        self.subscriber_slots.iter().for_each(|slot| {
            if let Some(subscription) = slot.upgrade().filter(|s| s.is_active()) {
                subscription.push_event(event);
            }
        });
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscriber_slots.iter().filter(|slot| slot.is_alive()).count()
    }
}

impl<T: Send + Copy + 'static, const MAX_SUBSCRIBERS: usize> Default for StreamProducer<T, MAX_SUBSCRIBERS> {
    fn default() -> Self {
        Self::new(256)
    }
}

unsafe impl<T: Send + Copy + 'static, const MAX_SUBSCRIBERS: usize> Send for StreamProducer<T, MAX_SUBSCRIBERS> {}
unsafe impl<T: Send + Copy + 'static, const MAX_SUBSCRIBERS: usize> Sync for StreamProducer<T, MAX_SUBSCRIBERS> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_subscription_push_pop() {
        let subscription = EventSubscription::<i32>::new(16);
        assert!(subscription.push_event(42));
        assert!(subscription.push_event(100));
        assert_eq!(subscription.pop_event(), Some(42));
        assert_eq!(subscription.pop_event(), Some(100));
        assert_eq!(subscription.pop_event(), None);
    }

    #[test]
    fn test_subscription_unsubscribe_stops_push() {
        let subscription = EventSubscription::<i32>::new(16);
        assert!(subscription.push_event(1));
        subscription.unsubscribe();
        assert!(!subscription.push_event(2));
        assert!(!subscription.is_active());
    }

    #[test]
    fn test_subscription_wait_immediate_return() {
        let subscription = EventSubscription::<i32>::new(16);
        subscription.push_event(42);
        assert_eq!(subscription.wait_for_events(1000), WaitResult::EventsAvailable);
    }

    #[test]
    fn test_subscription_wait_timeout() {
        let subscription = EventSubscription::<i32>::new(16);
        assert_eq!(subscription.wait_for_events(10), WaitResult::Timeout);
    }

    #[test]
    fn test_subscription_cross_thread() {
        use std::sync::Arc;

        let subscription = Arc::new(EventSubscription::<i32>::new(1024));
        let producer_subscription = Arc::clone(&subscription);

        let producer_thread = thread::spawn(move || {
            (0..100).for_each(|index| {
                producer_subscription.push_event(index);
                thread::sleep(Duration::from_micros(100));
            });
        });

        let mut received_events = Vec::new();
        while received_events.len() < 100 {
            let wait_result = subscription.wait_for_events(100);
            if wait_result == WaitResult::Unsubscribed {
                break;
            }

            while let Some(event) = subscription.pop_event() {
                received_events.push(event);
            }
        }

        producer_thread.join().unwrap();
        assert_eq!(received_events.len(), 100);
        assert!(received_events.iter().enumerate().all(|(index, &value)| value == index as i32));
    }
}
