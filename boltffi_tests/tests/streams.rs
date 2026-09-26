use std::sync::Arc;
use std::thread;
use std::time::Duration;

use boltffi::__private::WaitResult;
use boltffi_tests::*;

mod ring_buffer_is_fifo {
    use super::*;

    #[test]
    fn pop_returns_oldest_event_first() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        stream.emit(1);
        stream.emit(2);
        stream.emit(3);

        assert_eq!(sub.pop_event(), Some(1));
        assert_eq!(sub.pop_event(), Some(2));
        assert_eq!(sub.pop_event(), Some(3));
    }

    #[test]
    fn batch_pop_preserves_insertion_order() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        stream.emit(10);
        stream.emit(20);
        stream.emit(30);

        let mut buffer = [std::mem::MaybeUninit::<i32>::uninit(); 3];
        let count = sub.pop_batch_into(&mut buffer);

        let values: Vec<i32> = buffer[..count]
            .iter()
            .map(|v| unsafe { v.assume_init() })
            .collect();

        assert_eq!(values, vec![10, 20, 30]);
    }
}

mod each_subscriber_has_own_buffer {
    use super::*;

    #[test]
    fn all_subscribers_receive_all_events() {
        let stream = CounterStream::new();
        let sub1 = stream.subscribe();
        let sub2 = stream.subscribe();

        stream.emit(42);

        assert_eq!(sub1.pop_event(), Some(42));
        assert_eq!(sub2.pop_event(), Some(42));
    }

    #[test]
    fn consuming_from_one_does_not_affect_other() {
        let stream = CounterStream::new();
        let sub1 = stream.subscribe();
        let sub2 = stream.subscribe();

        stream.emit(1);
        stream.emit(2);

        sub1.pop_event();
        sub1.pop_event();

        assert_eq!(sub2.pop_event(), Some(1));
        assert_eq!(sub2.pop_event(), Some(2));
    }
}

mod active_state_controls_event_delivery {
    use super::*;

    #[test]
    fn new_subscription_is_active() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        assert!(sub.is_active());
    }

    #[test]
    fn unsubscribe_sets_inactive() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        sub.unsubscribe();

        assert!(!sub.is_active());
    }

    #[test]
    fn inactive_subscription_does_not_receive_new_events() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        sub.unsubscribe();
        stream.emit(999);

        assert_eq!(sub.pop_event(), None);
    }
}

mod wait_semantics {
    use super::*;

    #[test]
    fn returns_immediately_if_events_available() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        stream.emit(1);

        let start = std::time::Instant::now();
        let result = sub.wait_for_events(5000);

        assert_eq!(result, WaitResult::EventsAvailable);
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn returns_immediately_if_inactive() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        sub.unsubscribe();

        let result = sub.wait_for_events(5000);

        assert_eq!(result, WaitResult::Unsubscribed);
    }

    #[test]
    fn blocks_until_timeout_if_no_data() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        let timeout_ms = 50u32;
        let start = std::time::Instant::now();
        let result = sub.wait_for_events(timeout_ms);

        assert_eq!(result, WaitResult::Timeout);
        assert!(start.elapsed() >= Duration::from_millis(timeout_ms as u64 - 10));
    }

    #[test]
    fn wakes_when_event_arrives() {
        let stream = Arc::new(CounterStream::new());
        let sub = stream.subscribe();

        let stream_clone = Arc::clone(&stream);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            stream_clone.emit(123);
        });

        let result = sub.wait_for_events(2000);

        assert_eq!(result, WaitResult::EventsAvailable);
    }
}

mod pop_on_empty_buffer {
    use super::*;

    #[test]
    fn single_pop_returns_none() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        assert_eq!(sub.pop_event(), None);
    }

    #[test]
    fn batch_pop_returns_zero() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        let mut buffer = [std::mem::MaybeUninit::<i32>::uninit(); 10];
        let count = sub.pop_batch_into(&mut buffer);

        assert_eq!(count, 0);
    }
}

mod batch_pop_respects_buffer_capacity {
    use super::*;

    #[test]
    fn returns_min_of_available_and_capacity() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        stream.emit_batch(vec![1, 2, 3, 4, 5]);

        let mut buffer = [std::mem::MaybeUninit::<i32>::uninit(); 2];
        let count = sub.pop_batch_into(&mut buffer);

        assert_eq!(count, 2);
    }

    #[test]
    fn remaining_events_still_available() {
        let stream = CounterStream::new();
        let sub = stream.subscribe();

        stream.emit_batch(vec![1, 2, 3, 4, 5]);

        let mut buffer = [std::mem::MaybeUninit::<i32>::uninit(); 2];
        sub.pop_batch_into(&mut buffer);

        assert_eq!(sub.pop_event(), Some(3));
        assert_eq!(sub.pop_event(), Some(4));
        assert_eq!(sub.pop_event(), Some(5));
    }
}

mod struct_values_work {
    use super::*;

    #[test]
    fn struct_preserved_through_stream() {
        let stream = PointStream::new();
        let sub = stream.subscribe();

        let point = FixturePoint { x: 1.5, y: 2.5 };
        stream.emit(point);

        let received = sub.pop_event().unwrap();
        assert_eq!(received.x, 1.5);
        assert_eq!(received.y, 2.5);
    }
}

mod concurrent_access_is_safe {
    use super::*;

    #[test]
    fn producer_and_consumer_on_different_threads() {
        let stream = Arc::new(CounterStream::new());
        let sub = stream.subscribe();

        let stream_clone = Arc::clone(&stream);
        let producer = thread::spawn(move || {
            for i in 0..100 {
                stream_clone.emit(i);
            }
        });

        let consumer = thread::spawn(move || {
            let mut count = 0;
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while count < 100 && std::time::Instant::now() < deadline {
                if sub.pop_event().is_some() {
                    count += 1;
                } else {
                    thread::yield_now();
                }
            }
            count
        });

        producer.join().unwrap();
        let received = consumer.join().unwrap();

        assert_eq!(received, 100);
    }
}

mod a_failed_stream_reports_its_error_after_its_items {
    use super::*;
    use boltffi::__private::FfiBuf;

    /// A subscription handle as the generated `subscribe` export hands it out.
    fn handle(stream: &JobStream) -> u64 {
        Arc::into_raw(stream.subscribe()) as u64
    }

    fn take_error(handle: u64) -> Vec<u8> {
        unsafe {
            boltffi_stream_boltffi_tests_streams_job_stream_subscribe_take_error(handle).into_vec()
        }
    }

    fn pop(handle: u64) -> Vec<i32> {
        let mut items = [0_i32; 16];
        let count = unsafe {
            boltffi_stream_boltffi_tests_streams_job_stream_subscribe_pop_batch(
                handle,
                items.as_mut_ptr(),
                items.len(),
            )
        };
        items[..count].to_vec()
    }

    #[test]
    fn the_error_follows_the_buffered_items() {
        let stream = JobStream::new();
        let handle = handle(&stream);
        assert!(stream.emit(7));
        assert!(stream.fail("boom".to_owned()));

        let wait =
            unsafe { boltffi_stream_boltffi_tests_streams_job_stream_subscribe_wait(handle, 0) };
        assert_eq!(wait, WaitResult::Unsubscribed as i32);
        assert_eq!(pop(handle), vec![7]);

        let expected = unsafe { FfiBuf::wire_encode(&"boom".to_owned()).into_vec::<u8>() };
        assert_eq!(take_error(handle), expected);
        assert!(take_error(handle).is_empty(), "the error is taken once");

        unsafe { boltffi_stream_boltffi_tests_streams_job_stream_subscribe_free(handle) };
    }

    #[test]
    fn a_completed_or_cancelled_stream_has_no_error() {
        let stream = JobStream::new();
        let handle = handle(&stream);
        unsafe { boltffi_stream_boltffi_tests_streams_job_stream_subscribe_unsubscribe(handle) };

        assert!(!stream.fail("too late".to_owned()));
        assert!(take_error(handle).is_empty());

        unsafe { boltffi_stream_boltffi_tests_streams_job_stream_subscribe_free(handle) };
    }
}
