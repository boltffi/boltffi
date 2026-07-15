use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::Duration;
use std::{marker::PhantomData, mem::MaybeUninit};

use super::continuation::{ContinuationScheduler, ContinuationSignalPolicy};
use crate::ringbuffer::SpscRingBuffer;

#[repr(i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPollResult {
    Ready = 0,
    Closed = 1,
}

pub type StreamContinuationCallback = extern "C" fn(callback_data: u64, StreamPollResult);

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn __boltffi_stream_wake(subscription_handle: u32, result: StreamPollResult);
}

#[cfg(target_arch = "wasm32")]
extern "C" fn wasm_stream_wake(callback_data: u64, result: StreamPollResult) {
    unsafe { __boltffi_stream_wake(callback_data as u32, result) };
}

struct StreamContinuationPolicy;

impl ContinuationSignalPolicy for StreamContinuationPolicy {
    type Signal = StreamPollResult;

    fn displaced() -> Self::Signal {
        StreamPollResult::Ready
    }

    fn wake() -> Self::Signal {
        StreamPollResult::Ready
    }

    fn cancelled() -> Self::Signal {
        StreamPollResult::Closed
    }
}

pub struct EventSubscription<T: Send + 'static> {
    ring_buffer: SpscRingBuffer<T>,
    is_active: AtomicBool,
    notification_mutex: Mutex<()>,
    notification_condvar: Condvar,
    continuation_scheduler: ContinuationScheduler<StreamContinuationPolicy>,
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
            continuation_scheduler: ContinuationScheduler::new(),
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

        let wait_result = self.notification_condvar.wait_timeout_while(
            notification_guard,
            timeout_duration,
            |_| self.is_active() && self.ring_buffer.is_empty(),
        );

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

        self.continuation_scheduler
            .store_continuation(callback, callback_data);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn poll_wasm(&self, subscription_handle: u32) {
        self.poll(u64::from(subscription_handle), wasm_stream_wake);
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

pub type SubscriptionHandle = *mut c_void;

pub struct SubscriptionHandleAccess<T: Send + 'static> {
    subscription_handle: SubscriptionHandle,
    subscription_type: PhantomData<T>,
}

impl<T: Send + 'static> SubscriptionHandleAccess<T> {
    #[inline]
    pub fn allocate(capacity: usize) -> Self {
        let subscription = Box::new(EventSubscription::<T>::new(capacity));
        Self {
            subscription_handle: Box::into_raw(subscription).cast::<c_void>(),
            subscription_type: PhantomData,
        }
    }

    #[inline]
    pub fn from_raw(subscription_handle: SubscriptionHandle) -> Self {
        Self {
            subscription_handle,
            subscription_type: PhantomData,
        }
    }

    #[inline]
    pub fn raw_handle(&self) -> SubscriptionHandle {
        self.subscription_handle
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.subscription_handle.is_null()
    }

    #[inline]
    pub fn push(&self, event: T) -> bool {
        self.with_subscription(|subscription| subscription.push_event(event))
            .unwrap_or(false)
    }

    #[inline]
    pub fn wait(&self, timeout_milliseconds: u32) -> WaitResult {
        self.with_subscription(|subscription| subscription.wait_for_events(timeout_milliseconds))
            .unwrap_or(WaitResult::Unsubscribed)
    }

    #[inline]
    pub fn poll(&self, callback_data: u64, callback: StreamContinuationCallback) {
        self.with_subscription(|subscription| subscription.poll(callback_data, callback))
            .unwrap_or_else(|| callback(callback_data, StreamPollResult::Closed));
    }

    #[inline]
    pub fn unsubscribe(&self) {
        self.with_subscription(EventSubscription::unsubscribe);
    }

    #[inline]
    pub fn free(self) {
        drop(self.into_owned_subscription());
    }

    #[inline]
    pub fn pop_batch_into(&self, output_buffer: &mut [MaybeUninit<T>]) -> usize
    where
        T: Copy,
    {
        self.with_subscription(|subscription| subscription.pop_batch_into(output_buffer))
            .unwrap_or(0)
    }

    #[inline]
    fn with_subscription<Result>(
        &self,
        subscription_access: impl FnOnce(&EventSubscription<T>) -> Result,
    ) -> Option<Result> {
        (!self.subscription_handle.is_null()).then(|| {
            let subscription = unsafe { &*self.subscription_handle.cast::<EventSubscription<T>>() };
            subscription_access(subscription)
        })
    }

    #[inline]
    fn into_owned_subscription(self) -> Option<Box<EventSubscription<T>>> {
        (!self.subscription_handle.is_null()).then(|| unsafe {
            Box::from_raw(self.subscription_handle.cast::<EventSubscription<T>>())
        })
    }
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

        match self.weak_ptr.compare_exchange(
            std::ptr::null_mut(),
            raw_ptr,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
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

        let successfully_cleared = is_dead
            && self
                .weak_ptr
                .compare_exchange(
                    ptr,
                    std::ptr::null_mut(),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok();

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

pub struct StreamProducer<T: Send + Clone + 'static, const MAX_SUBSCRIBERS: usize = 32> {
    subscriber_slots: [SubscriberSlot<T>; MAX_SUBSCRIBERS],
    default_capacity: usize,
}

impl<T: Send + Clone + 'static, const MAX_SUBSCRIBERS: usize> StreamProducer<T, MAX_SUBSCRIBERS> {
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

        self.subscriber_slots
            .iter()
            .for_each(|slot| slot.clear_if_dead());

        let slot_claimed = self
            .subscriber_slots
            .iter()
            .any(|slot| slot.try_claim(&subscription));

        if !slot_claimed {
            eprintln!(
                "StreamProducer: all {} subscriber slots full",
                MAX_SUBSCRIBERS
            );
        }

        subscription
    }

    pub fn push(&self, event: T) {
        self.subscriber_slots.iter().for_each(|slot| {
            if let Some(subscription) = slot.upgrade().filter(|s| s.is_active()) {
                subscription.push_event(event.clone());
            }
        });
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscriber_slots
            .iter()
            .filter(|slot| slot.is_alive())
            .count()
    }
}

impl<T: Send + Clone + 'static, const MAX_SUBSCRIBERS: usize> Default
    for StreamProducer<T, MAX_SUBSCRIBERS>
{
    fn default() -> Self {
        Self::new(256)
    }
}

unsafe impl<T: Send + Clone + 'static, const MAX_SUBSCRIBERS: usize> Send
    for StreamProducer<T, MAX_SUBSCRIBERS>
{
}
unsafe impl<T: Send + Clone + 'static, const MAX_SUBSCRIBERS: usize> Sync
    for StreamProducer<T, MAX_SUBSCRIBERS>
{
}

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
        assert_eq!(
            subscription.wait_for_events(1000),
            WaitResult::EventsAvailable
        );
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
        assert!(
            received_events
                .iter()
                .enumerate()
                .all(|(index, &value)| value == index as i32)
        );
    }

    #[test]
    fn test_stream_producer_broadcasts_clone_items() {
        let producer = StreamProducer::<String>::new(16);
        let first = producer.subscribe();
        let second = producer.subscribe();

        producer.push("hello".to_string());

        assert_eq!(first.pop_event(), Some("hello".to_string()));
        assert_eq!(second.pop_event(), Some("hello".to_string()));
    }
}
