use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use boltffi_tests::*;

struct InvocationCounter {
    count: Cell<u32>,
    return_value: i32,
}

impl InvocationCounter {
    fn new(return_value: i32) -> Self {
        Self {
            count: Cell::new(0),
            return_value,
        }
    }

    fn invocations(&self) -> u32 {
        self.count.get()
    }
}

impl SyncValueCallback for InvocationCounter {
    fn on_value(&self, _value: i32) -> i32 {
        self.count.set(self.count.get() + 1);
        self.return_value
    }
}

impl SyncValueCallback for &InvocationCounter {
    fn on_value(&self, value: i32) -> i32 {
        (*self).on_value(value)
    }
}

struct Transformer {
    multiplier: i32,
    offset: i32,
}

impl SyncValueCallback for Transformer {
    fn on_value(&self, value: i32) -> i32 {
        value * self.multiplier + self.offset
    }
}

struct FixedProvider {
    points: Vec<FixturePoint>,
}

impl SyncDataProvider for FixedProvider {
    fn get_count(&self) -> u32 {
        self.points.len() as u32
    }

    fn get_item(&self, index: u32) -> FixturePoint {
        self.points[index as usize]
    }
}

struct VecDoubler;

impl SyncVecCallback for VecDoubler {
    fn on_vec(&self, values: Vec<i32>) -> Vec<i32> {
        values.into_iter().map(|v| v * 2).collect()
    }
}

struct PointScaler {
    scale: f64,
}

impl SyncStructCallback for PointScaler {
    fn on_struct(&self, point: FixturePoint) -> FixturePoint {
        FixturePoint {
            x: point.x * self.scale,
            y: point.y * self.scale,
        }
    }
}

struct ThresholdFinder {
    threshold: i32,
}

impl SyncOptionCallback for ThresholdFinder {
    fn find_value(&self, key: i32) -> Option<i32> {
        if key > self.threshold {
            Some(key * 10)
        } else {
            None
        }
    }
}

struct StatusMapper;

impl SyncEnumCallback for StatusMapper {
    fn get_status(&self, id: i32) -> FixtureStatus {
        match id % 4 {
            0 => FixtureStatus::Pending,
            1 => FixtureStatus::Active,
            2 => FixtureStatus::Completed,
            _ => FixtureStatus::Failed,
        }
    }
}

struct MultiMethodImpl {
    base: i32,
}

impl SyncMultiMethodCallback for MultiMethodImpl {
    fn method_a(&self, x: i32) -> i32 {
        x + self.base
    }

    fn method_b(&self, x: i32, y: i32) -> i32 {
        x * y + self.base
    }

    fn method_c(&self) -> i32 {
        self.base * 2
    }
}

struct CountingFetcher {
    fetch_count: Arc<AtomicU32>,
    base_value: u64,
}

impl CountingFetcher {
    fn new(base_value: u64) -> Self {
        Self {
            fetch_count: Arc::new(AtomicU32::new(0)),
            base_value,
        }
    }

    fn fetch_count(&self) -> u32 {
        self.fetch_count.load(Ordering::SeqCst)
    }
}

impl AsyncFetcher for CountingFetcher {
    async fn fetch(&self, key: u32) -> u64 {
        self.fetch_count.fetch_add(1, Ordering::SeqCst);
        self.base_value + key as u64
    }
}

impl AsyncFetcher for &CountingFetcher {
    async fn fetch(&self, key: u32) -> u64 {
        (*self).fetch(key).await
    }
}

struct AsyncOptionImpl {
    threshold: i32,
}

impl AsyncOptionFetcher for AsyncOptionImpl {
    async fn find(&self, key: i32) -> Option<i64> {
        if key > self.threshold {
            Some(key as i64 * 100)
        } else {
            None
        }
    }
}

impl AsyncOptionFetcher for &AsyncOptionImpl {
    async fn find(&self, key: i32) -> Option<i64> {
        (*self).find(key).await
    }
}

struct AsyncMultiImpl {
    base: i64,
}

impl AsyncMultiMethod for AsyncMultiImpl {
    async fn load(&self, id: i64) -> i64 {
        id + self.base
    }

    async fn compute(&self, a: i32, b: i32) -> i64 {
        (a as i64) * (b as i64) + self.base
    }
}

impl AsyncMultiMethod for &AsyncMultiImpl {
    async fn load(&self, id: i64) -> i64 {
        (*self).load(id).await
    }

    async fn compute(&self, a: i32, b: i32) -> i64 {
        (*self).compute(a, b).await
    }
}

mod foreign_type_generation {
    use super::*;

    fn assert_type_exists<T>() {}

    #[test]
    fn sync_value_callback_generates_foreign_type() {
        assert_type_exists::<ForeignSyncValueCallback>();
    }

    #[test]
    fn sync_data_provider_generates_foreign_type() {
        assert_type_exists::<ForeignSyncDataProvider>();
    }

    #[test]
    fn sync_vec_callback_generates_foreign_type() {
        assert_type_exists::<ForeignSyncVecCallback>();
    }

    #[test]
    fn sync_struct_callback_generates_foreign_type() {
        assert_type_exists::<ForeignSyncStructCallback>();
    }

    #[test]
    fn sync_option_callback_generates_foreign_type() {
        assert_type_exists::<ForeignSyncOptionCallback>();
    }

    #[test]
    fn sync_enum_callback_generates_foreign_type() {
        assert_type_exists::<ForeignSyncEnumCallback>();
    }

    #[test]
    fn sync_multi_method_generates_foreign_type() {
        assert_type_exists::<ForeignSyncMultiMethodCallback>();
    }

    #[test]
    fn async_fetcher_generates_foreign_type() {
        assert_type_exists::<ForeignAsyncFetcher>();
    }

    #[test]
    fn async_option_fetcher_generates_foreign_type() {
        assert_type_exists::<ForeignAsyncOptionFetcher>();
    }

    #[test]
    fn async_multi_method_generates_foreign_type() {
        assert_type_exists::<ForeignAsyncMultiMethod>();
    }
}

mod sync_value_callback {
    use super::*;

    #[test]
    fn boxed_returns_correct_value() {
        let cb = Transformer {
            multiplier: 2,
            offset: 5,
        };
        assert_eq!(invoke_sync_boxed(Box::new(cb), 10), 25);
    }

    #[test]
    fn impl_returns_correct_value() {
        let cb = Transformer {
            multiplier: 3,
            offset: 7,
        };
        assert_eq!(invoke_sync_impl(cb, 10), 37);
    }

    #[test]
    fn impl_invokes_exactly_once() {
        let counter = InvocationCounter::new(42);
        let _ = invoke_sync_impl(&counter, 0);
        assert_eq!(counter.invocations(), 1);
    }

    #[test]
    fn handles_negative_input() {
        let cb = Transformer {
            multiplier: 2,
            offset: 0,
        };
        assert_eq!(invoke_sync_boxed(Box::new(cb), -10), -20);
    }

    #[test]
    fn handles_zero_input() {
        let cb = Transformer {
            multiplier: 999,
            offset: 42,
        };
        assert_eq!(invoke_sync_impl(cb, 0), 42);
    }

    #[test]
    fn handles_large_values() {
        let cb = Transformer {
            multiplier: 1,
            offset: 0,
        };
        assert_eq!(invoke_sync_boxed(Box::new(cb), i32::MAX), i32::MAX);
    }
}

mod sync_data_provider {
    use super::*;

    #[test]
    fn boxed_empty_returns_zero() {
        let provider = FixedProvider { points: vec![] };
        assert_eq!(sum_provider_boxed(Box::new(provider)), 0.0);
    }

    #[test]
    fn impl_single_point() {
        let provider = FixedProvider {
            points: vec![FixturePoint { x: 1.5, y: 2.5 }],
        };
        assert_eq!(sum_provider_impl(provider), 4.0);
    }

    #[test]
    fn boxed_multiple_points() {
        let provider = FixedProvider {
            points: vec![
                FixturePoint { x: 1.0, y: 2.0 },
                FixturePoint { x: 3.0, y: 4.0 },
            ],
        };
        assert_eq!(sum_provider_boxed(Box::new(provider)), 10.0);
    }

    #[test]
    fn handles_negative_coordinates() {
        let provider = FixedProvider {
            points: vec![
                FixturePoint { x: -5.0, y: 5.0 },
                FixturePoint { x: 5.0, y: -5.0 },
            ],
        };
        assert_eq!(sum_provider_impl(provider), 0.0);
    }
}

mod sync_vec_callback {
    use super::*;

    #[test]
    fn boxed_transforms_vec() {
        let cb = VecDoubler;
        let result = invoke_vec_boxed(Box::new(cb), vec![1, 2, 3]);
        assert_eq!(result, vec![2, 4, 6]);
    }

    #[test]
    fn impl_transforms_vec() {
        let cb = VecDoubler;
        let result = invoke_vec_impl(cb, vec![10, 20, 30]);
        assert_eq!(result, vec![20, 40, 60]);
    }

    #[test]
    fn empty_vec_returns_empty() {
        let cb = VecDoubler;
        let result = invoke_vec_boxed(Box::new(cb), vec![]);
        assert!(result.is_empty());
    }
}

mod sync_struct_callback {
    use super::*;

    #[test]
    fn boxed_transforms_struct() {
        let cb = PointScaler { scale: 2.0 };
        let result = invoke_struct_boxed(Box::new(cb), FixturePoint { x: 3.0, y: 4.0 });
        assert_eq!(result, FixturePoint { x: 6.0, y: 8.0 });
    }

    #[test]
    fn impl_transforms_struct() {
        let cb = PointScaler { scale: 0.5 };
        let result = invoke_struct_impl(cb, FixturePoint { x: 10.0, y: 20.0 });
        assert_eq!(result, FixturePoint { x: 5.0, y: 10.0 });
    }

    #[test]
    fn handles_zero_point() {
        let cb = PointScaler { scale: 100.0 };
        let result = invoke_struct_boxed(Box::new(cb), FixturePoint { x: 0.0, y: 0.0 });
        assert_eq!(result, FixturePoint { x: 0.0, y: 0.0 });
    }
}

mod sync_option_callback {
    use super::*;

    #[test]
    fn boxed_returns_some() {
        let cb = ThresholdFinder { threshold: 5 };
        assert_eq!(invoke_option_boxed(Box::new(cb), 10), Some(100));
    }

    #[test]
    fn impl_returns_none() {
        let cb = ThresholdFinder { threshold: 100 };
        assert_eq!(invoke_option_impl(cb, 50), None);
    }

    #[test]
    fn boundary_at_threshold() {
        let cb = ThresholdFinder { threshold: 10 };
        assert_eq!(invoke_option_boxed(Box::new(cb), 10), None);
    }

    #[test]
    fn boundary_above_threshold() {
        let cb = ThresholdFinder { threshold: 10 };
        assert_eq!(invoke_option_impl(cb, 11), Some(110));
    }
}

mod sync_enum_callback {
    use super::*;

    #[test]
    fn boxed_returns_pending() {
        let cb = StatusMapper;
        assert_eq!(invoke_enum_boxed(Box::new(cb), 0), FixtureStatus::Pending);
    }

    #[test]
    fn impl_returns_active() {
        let cb = StatusMapper;
        assert_eq!(invoke_enum_impl(cb, 1), FixtureStatus::Active);
    }

    #[test]
    fn returns_completed() {
        let cb = StatusMapper;
        assert_eq!(invoke_enum_boxed(Box::new(cb), 2), FixtureStatus::Completed);
    }

    #[test]
    fn returns_failed() {
        let cb = StatusMapper;
        assert_eq!(invoke_enum_impl(cb, 3), FixtureStatus::Failed);
    }

    #[test]
    fn cycles_through_variants() {
        let cb = StatusMapper;
        assert_eq!(invoke_enum_boxed(Box::new(cb), 4), FixtureStatus::Pending);
        assert_eq!(
            invoke_enum_boxed(Box::new(StatusMapper), 5),
            FixtureStatus::Active
        );
    }
}

mod sync_multi_method_callback {
    use super::*;

    #[test]
    fn boxed_calls_all_methods() {
        let cb = MultiMethodImpl { base: 10 };
        let result = invoke_multi_method_boxed(Box::new(cb), 5, 3);
        assert_eq!(result, (5 + 10) + (5 * 3 + 10) + (10 * 2));
    }

    #[test]
    fn impl_calls_all_methods() {
        let cb = MultiMethodImpl { base: 0 };
        let result = invoke_multi_method_impl(cb, 4, 6);
        assert_eq!(result, 28);
    }
}

mod multiple_sync_callbacks {
    use super::*;

    #[test]
    fn two_impl_both_invoked() {
        let first = InvocationCounter::new(10);
        let second = InvocationCounter::new(20);
        let result = invoke_two_sync_impl(&first, &second, 5);
        assert_eq!(first.invocations(), 1);
        assert_eq!(second.invocations(), 1);
        assert_eq!(result, 30);
    }

    #[test]
    fn three_impl_all_invoked() {
        let first = InvocationCounter::new(1);
        let second = InvocationCounter::new(2);
        let third = InvocationCounter::new(3);
        let result = invoke_three_sync_impl(&first, &second, &third, 0);
        assert_eq!(first.invocations(), 1);
        assert_eq!(second.invocations(), 1);
        assert_eq!(third.invocations(), 1);
        assert_eq!(result, 6);
    }

    #[test]
    fn mixed_boxed_and_impl() {
        let boxed = Transformer {
            multiplier: 2,
            offset: 0,
        };
        let impl_cb = Transformer {
            multiplier: 3,
            offset: 0,
        };
        let result = invoke_mixed_sync(Box::new(boxed), impl_cb, 10);
        assert_eq!(result, 20 * 30);
    }

    #[test]
    fn mixed_three_all_invoked() {
        let boxed = Transformer {
            multiplier: 1,
            offset: 10,
        };
        let impl1 = Transformer {
            multiplier: 1,
            offset: 20,
        };
        let impl2 = Transformer {
            multiplier: 1,
            offset: 30,
        };
        let result = invoke_mixed_three(Box::new(boxed), impl1, impl2, 0);
        assert_eq!(result, 60);
    }
}

mod async_fetcher {
    use super::*;

    #[tokio::test]
    async fn impl_returns_correct_value() {
        let fetcher = CountingFetcher::new(100);
        let result = invoke_async_impl(&fetcher, 5).await;
        assert_eq!(result, 105);
    }

    #[tokio::test]
    async fn impl_invoked_exactly_once() {
        let fetcher = CountingFetcher::new(0);
        let _ = invoke_async_impl(&fetcher, 42).await;
        assert_eq!(fetcher.fetch_count(), 1);
    }

    #[tokio::test]
    async fn two_impl_both_invoked() {
        let first = CountingFetcher::new(10);
        let second = CountingFetcher::new(20);
        let result = invoke_two_async_impl(&first, &second, 5).await;
        assert_eq!(first.fetch_count(), 1);
        assert_eq!(second.fetch_count(), 1);
        assert_eq!(result, 15 * 25);
    }

    #[tokio::test]
    async fn three_impl_all_invoked() {
        let first = CountingFetcher::new(1);
        let second = CountingFetcher::new(2);
        let third = CountingFetcher::new(3);
        let result = invoke_three_async_impl(&first, &second, &third, 10).await;
        assert_eq!(first.fetch_count(), 1);
        assert_eq!(second.fetch_count(), 1);
        assert_eq!(third.fetch_count(), 1);
        assert_eq!(result, 11 + 12 + 13);
    }
}

mod async_option_fetcher {
    use super::*;

    #[tokio::test]
    async fn impl_returns_some() {
        let fetcher = AsyncOptionImpl { threshold: 5 };
        let result = invoke_async_option_impl(&fetcher, 10).await;
        assert_eq!(result, Some(1000));
    }

    #[tokio::test]
    async fn impl_returns_none() {
        let fetcher = AsyncOptionImpl { threshold: 100 };
        let result = invoke_async_option_impl(&fetcher, 50).await;
        assert_eq!(result, None);
    }
}

mod async_multi_method {
    use super::*;

    #[tokio::test]
    async fn impl_calls_both_methods() {
        let cb = AsyncMultiImpl { base: 10 };
        let result = invoke_async_multi_impl(&cb, 5, 3, 4).await;
        assert_eq!(result, (5 + 10) + (3 * 4 + 10));
    }
}

mod struct_method_callbacks {
    use super::*;

    #[test]
    fn processor_apply_impl() {
        let processor = SyncProcessor::new(3);
        let cb = Transformer {
            multiplier: 2,
            offset: 1,
        };
        let result = processor.apply_impl(cb, 5);
        assert_eq!(result, (5 * 3) * 2 + 1);
    }

    #[test]
    fn processor_apply_boxed() {
        let processor = SyncProcessor::new(4);
        let cb = Transformer {
            multiplier: 1,
            offset: 0,
        };
        let result = processor.apply_boxed(Box::new(cb), 10);
        assert_eq!(result, 40);
    }

    #[test]
    fn processor_apply_struct_impl() {
        let processor = SyncProcessor::new(2);
        let cb = PointScaler { scale: 3.0 };
        let result = processor.apply_struct_impl(cb, FixturePoint { x: 1.0, y: 2.0 });
        assert_eq!(result, FixturePoint { x: 6.0, y: 12.0 });
    }

    #[test]
    fn processor_apply_option_impl_some() {
        let processor = SyncProcessor::new(2);
        let cb = ThresholdFinder { threshold: 5 };
        let result = processor.apply_option_impl(cb, 10);
        assert_eq!(result, Some(200));
    }

    #[test]
    fn processor_apply_option_impl_none() {
        let processor = SyncProcessor::new(1);
        let cb = ThresholdFinder { threshold: 100 };
        let result = processor.apply_option_impl(cb, 50);
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn async_processor_fetch_with_offset() {
        let processor = AsyncProcessor::new(100);
        let fetcher = CountingFetcher::new(50);
        let result = processor.fetch_with_offset(&fetcher, 10).await;
        assert_eq!(result, 160);
    }

    #[tokio::test]
    async fn async_processor_find_with_offset_some() {
        let processor = AsyncProcessor::new(5);
        let fetcher = AsyncOptionImpl { threshold: 0 };
        let result = processor.find_with_offset(&fetcher, 10).await;
        assert_eq!(result, Some(1005));
    }

    #[tokio::test]
    async fn async_processor_find_with_offset_none() {
        let processor = AsyncProcessor::new(100);
        let fetcher = AsyncOptionImpl { threshold: 50 };
        let result = processor.find_with_offset(&fetcher, 25).await;
        assert_eq!(result, None);
    }
}

mod async_fetcher_foreign_failure {
    use super::*;
    use boltffi::__private::{BoxFromCallbackHandle, FfiStatus};
    use std::ffi::c_void;

    extern "C" fn free_handle(_handle: u64) {}

    extern "C" fn clone_handle(handle: u64) -> u64 {
        handle
    }

    extern "C" fn failing_fetch(
        _handle: u64,
        _key: u32,
        completion: extern "C" fn(*mut c_void, FfiStatus, u64),
        completion_data: *mut c_void,
    ) {
        completion(completion_data, FfiStatus::INTERNAL_ERROR, 0);
    }

    static FAILING_VTABLE: AsyncFetcherVTable = AsyncFetcherVTable {
        free: free_handle,
        clone: clone_handle,
        fetch: failing_fetch,
    };

    #[tokio::test]
    async fn completion_failure_resolves_instead_of_panicking() {
        unsafe {
            boltffi_register_callback_boltffi_tests_callbacks_async_fetcher(&FAILING_VTABLE);
        }
        let handle = boltffi_create_callback_boltffi_tests_callbacks_async_fetcher(7);
        let fetcher = unsafe { ForeignAsyncFetcher::box_from_callback_handle(handle) };
        let result = invoke_async_impl(*fetcher, 5).await;
        assert_eq!(result, 0);
    }
}
