use boltffi::__private::rustfuture::{self, RustFuturePoll};
use boltffi::__private::{FfiBuf, RustFutureHandle};
use boltffi_core::wire::{WireDecode, WireEncode};
use boltffi_tests::*;

fn decode_buf<T: WireDecode>(buf: &FfiBuf) -> T {
    let (result, _) = T::decode_from(unsafe { buf.as_byte_slice() }).unwrap();
    result
}

fn decode_i32_vec(buf: FfiBuf) -> Vec<i32> {
    if buf.align() == core::mem::align_of::<i32>() {
        unsafe { buf.into_vec::<i32>() }
    } else {
        let (result, _) = Vec::<i32>::decode_from(unsafe { buf.as_byte_slice() }).unwrap();
        result
    }
}

fn encode<T: WireEncode>(value: &T) -> Vec<u8> {
    let size = value.wire_size();
    let mut buf = vec![0u8; size];
    value.encode_to(&mut buf);
    buf
}

fn encode_buf<T: WireEncode>(value: &T) -> FfiBuf {
    FfiBuf::wire_encode(value)
}

fn with_encoded<T: WireEncode, R>(value: &T, call: impl FnOnce(*const u8, usize) -> R) -> R {
    let buf = encode_buf(value);
    call(buf.as_ptr(), buf.len())
}

fn with_encoded_pair<A: WireEncode, B: WireEncode, R>(
    first: &A,
    second: &B,
    call: impl FnOnce(*const u8, usize, *const u8, usize) -> R,
) -> R {
    with_encoded(first, |first_ptr, first_len| {
        with_encoded(second, |second_ptr, second_len| {
            call(first_ptr, first_len, second_ptr, second_len)
        })
    })
}

fn with_encoded_str<R>(value: &str, call: impl FnOnce(*const u8, usize) -> R) -> R {
    with_encoded(&value.to_string(), call)
}

fn with_encoded_str_pair<R>(
    first: &str,
    second: &str,
    call: impl FnOnce(*const u8, usize, *const u8, usize) -> R,
) -> R {
    with_encoded_pair(&first.to_string(), &second.to_string(), call)
}

fn decode_out_result<T, E: WireDecode>(error: &FfiBuf, value: T) -> Result<T, E> {
    if error.is_empty() {
        Ok(value)
    } else {
        Err(decode_buf(error))
    }
}

fn try_new_fixture(id: i32) -> Result<u64, String> {
    let mut handle = 0;
    let error = unsafe {
        boltffi_init_class_boltffi_tests_classes_class_test_fixture_try_new(id, &mut handle)
    };
    decode_out_result(&error, handle)
}

fn try_get_fixture_value(handle: u64, index: i32) -> Result<i32, String> {
    let mut value = 0;
    let error = unsafe {
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_try_get_value(
            handle, index, &mut value,
        )
    };
    decode_out_result(&error, value)
}

fn try_parse_fixture(value: &str) -> Result<i32, String> {
    with_encoded_str(value, |ptr, len| {
        let mut parsed = 0;
        let error = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_try_parse(
                ptr,
                len,
                &mut parsed,
            )
        };
        decode_out_result(&error, parsed)
    })
}

fn get_fallible_service_value(handle: u64, key: i32) -> Result<i32, FixtureError> {
    let mut value = 0;
    let error = unsafe {
        boltffi_method_class_boltffi_tests_results_fallible_service_get_value(
            handle, key, &mut value,
        )
    };
    decode_out_result(&error, value)
}

fn add_marker(map: u64, id: i32) -> u64 {
    unsafe {
        boltffi_method_class_boltffi_tests_classes_fixture_map_add_marker(
            map,
            FixtureMarkerOptions { id },
        )
    }
}

fn default_marker(id: i32) -> u64 {
    unsafe {
        boltffi_method_class_boltffi_tests_classes_fixture_map_default_marker(
            FixtureMarkerOptions { id },
        )
    }
}

fn maybe_marker(map: u64, id: i32, should_create: bool) -> u64 {
    unsafe {
        boltffi_method_class_boltffi_tests_classes_fixture_map_maybe_marker(
            map,
            FixtureMarkerOptions { id },
            should_create,
        )
    }
}

fn new_fixture_with_name(name: &str) -> u64 {
    with_encoded_str(name, |ptr, len| unsafe {
        boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_name(ptr, len)
    })
}

fn new_fixture_with_point(point: FixturePoint) -> u64 {
    unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_point(point) }
}

fn new_full_fixture(id: i32, name: &str, point: FixturePoint, status: FixtureStatus) -> u64 {
    with_encoded_str(name, |name_ptr, name_len| unsafe {
        boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_full(
            id,
            name_ptr,
            name_len,
            point,
            status as i32,
        )
    })
}

fn get_fixture_point(handle: u64) -> FixturePoint {
    unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_point(handle) }
}

fn set_fixture_name(handle: u64, name: &str) {
    with_encoded_str(name, |ptr, len| unsafe {
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_name(handle, ptr, len)
    });
}

fn set_fixture_point(handle: u64, point: FixturePoint) {
    unsafe {
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_point(handle, point)
    };
}

fn values_near_point(handle: u64, point: FixturePoint) -> Vec<i32> {
    let buf = unsafe {
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_near_point(
            handle, point,
        )
    };
    decode_i32_vec(buf)
}

fn concat_strings(first: &str, second: &str) -> String {
    let buf = with_encoded_str_pair(
        first,
        second,
        |first_ptr, first_len, second_ptr, second_len| unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_concat(
                first_ptr, first_len, second_ptr, second_len,
            )
        },
    );
    decode_buf(&buf)
}

fn make_static_point(x: f64, y: f64) -> FixturePoint {
    unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_make_point(x, y) }
}

fn async_set_fixture_name(handle: u64, name: &str) -> RustFutureHandle {
    with_encoded_str(name, |ptr, len| unsafe {
        boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_set_name(
            handle, ptr, len,
        )
    })
}

mod constructor_and_free {
    use super::*;

    #[test]
    fn new_returns_valid_handle() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(42) };
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }

    #[test]
    fn multiple_handles_are_independent() {
        let h1 = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(10) };
        let h2 = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(20) };

        assert_ne!(h1, 0);
        assert_ne!(h2, 0);
        assert_ne!(h1, h2);

        unsafe {
            boltffi_release_class_boltffi_tests_classes_test_counter(h1);
            boltffi_release_class_boltffi_tests_classes_test_counter(h2);
        }
    }
}

mod ref_self_methods {
    use super::*;

    #[test]
    fn get_returns_initial_value() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(42) };
        let result = unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_get(handle) };
        assert_eq!(result, 42);
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }
}

mod ref_mut_self_methods {
    use super::*;

    #[test]
    fn set_modifies_value() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(0) };
        unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_set(handle, 100) };
        let result = unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_get(handle) };
        assert_eq!(result, 100);
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }

    #[test]
    fn add_modifies_and_returns_value() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(10) };
        let result =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_add(handle, 5) };
        assert_eq!(result, 15);
        let get_result =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_get(handle) };
        assert_eq!(get_result, 15);
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }
}

mod object_handle_returns {
    use super::*;

    #[test]
    fn instance_method_returns_single_threaded_class_handle() {
        let map = boltffi_init_class_boltffi_tests_classes_fixture_map_new();
        let marker = add_marker(map, 64);

        assert_ne!(marker, 0);
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_fixture_marker_id(marker) },
            64
        );

        unsafe {
            boltffi_release_class_boltffi_tests_classes_fixture_marker(marker);
            boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        }
    }

    #[test]
    fn static_method_returns_single_threaded_class_handle() {
        let marker = default_marker(128);

        assert_ne!(marker, 0);
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_fixture_marker_id(marker) },
            128
        );

        unsafe {
            boltffi_release_class_boltffi_tests_classes_fixture_marker(marker);
        }
    }

    #[test]
    fn self_return_lowers_to_single_threaded_class_handle() {
        let map = boltffi_init_class_boltffi_tests_classes_fixture_map_new();
        let cloned_map =
            unsafe { boltffi_method_class_boltffi_tests_classes_fixture_map_clone_handle(map) };

        assert_ne!(cloned_map, 0);

        unsafe {
            boltffi_release_class_boltffi_tests_classes_fixture_map(cloned_map);
            boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        }
    }

    #[test]
    fn optional_class_return_lowers_to_nullable_handle() {
        let map = boltffi_init_class_boltffi_tests_classes_fixture_map_new();
        let marker = maybe_marker(map, 256, true);
        let missing_marker = maybe_marker(map, 512, false);

        assert_ne!(marker, 0);
        assert_eq!(missing_marker, 0);
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_fixture_marker_id(marker) },
            256
        );

        unsafe {
            boltffi_release_class_boltffi_tests_classes_fixture_marker(marker);
            boltffi_release_class_boltffi_tests_classes_fixture_map(map);
        }
    }
}

mod async_ref_self_methods {
    use super::*;

    #[test]
    fn async_get_returns_future_handle() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(42) };
        let future =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_async_get(handle) };
        assert!(!future.is_null());
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_test_counter_async_get_free(future)
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }

    #[test]
    fn async_get_completes_with_value() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(42) };
        let future =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_async_get(handle) };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<i32>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete(future) };
        assert_eq!(result, Ok(42));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_test_counter_async_get_free(future)
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }
}

mod async_ref_mut_self_methods {
    use super::*;

    #[test]
    fn async_add_returns_future_handle() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(10) };
        let future =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_async_add(handle, 5) };
        assert!(!future.is_null());
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_free(future)
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }

    #[test]
    fn async_add_modifies_state_and_returns_result() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(10) };
        let future =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_async_add(handle, 7) };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<i32>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete(future) };
        assert_eq!(result, Ok(17));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_free(future)
        };

        let current =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_get(handle) };
        assert_eq!(current, 17);

        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }

    #[test]
    fn async_add_multiple_calls_accumulate() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_test_counter_new(0) };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}

        let f1 = unsafe {
            boltffi_method_class_boltffi_tests_classes_test_counter_async_add(handle, 10)
        };
        unsafe { rustfuture::rust_future_poll::<i32>(f1, noop, 0) };
        let r1 = unsafe { rustfuture::rust_future_complete(f1) };
        assert_eq!(r1, Ok(10));
        unsafe { boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_free(f1) };

        let f2 = unsafe {
            boltffi_method_class_boltffi_tests_classes_test_counter_async_add(handle, 20)
        };
        unsafe { rustfuture::rust_future_poll::<i32>(f2, noop, 0) };
        let r2 = unsafe { rustfuture::rust_future_complete(f2) };
        assert_eq!(r2, Ok(30));
        unsafe { boltffi_async_method_class_boltffi_tests_classes_test_counter_async_add_free(f2) };

        let final_value =
            unsafe { boltffi_method_class_boltffi_tests_classes_test_counter_get(handle) };
        assert_eq!(final_value, 30);

        unsafe { boltffi_release_class_boltffi_tests_classes_test_counter(handle) };
    }
}

mod fixture_constructors {
    use super::*;

    #[test]
    fn new_default_returns_valid_handle() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_default_initializes_id_to_zero() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            0
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_id_stores_value() {
        let handle =
            unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(42) };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            42
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_id_handles_negative() {
        let handle = unsafe {
            boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(-100)
        };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            -100
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_id_handles_max() {
        let handle = unsafe {
            boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(i32::MAX)
        };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            i32::MAX
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_id_handles_min() {
        let handle = unsafe {
            boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(i32::MIN)
        };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            i32::MIN
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn try_new_returns_handle_on_valid_input() {
        let handle = try_new_fixture(10).unwrap();
        assert_ne!(handle, 0);
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            10
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn try_new_returns_null_on_invalid_input() {
        let result = try_new_fixture(-1);
        assert!(result.is_err());
    }
}

mod fixture_ref_self_methods {
    use super::*;

    #[test]
    fn get_id_returns_stored_value() {
        let handle =
            unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(999) };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            999
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn values_count_empty_returns_zero() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(handle)
            },
            0
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn compute_sum_empty_returns_zero() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(handle)
            },
            0
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn compute_sum_with_values() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 20)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 30)
        };
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(handle)
            },
            60
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_ref_mut_self_methods {
    use super::*;

    #[test]
    fn set_id_modifies_value() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_id(handle, 777)
        };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            777
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn add_value_increments_count() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(handle)
            },
            1
        );
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 20)
        };
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(handle)
            },
            2
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn clear_values_resets_to_empty() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 20)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_clear_values(handle)
        };
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(handle)
            },
            0
        );
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(handle)
            },
            0
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_static_methods {
    use super::*;

    #[test]
    fn static_add_positive() {
        let result = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_add(3, 4)
        };
        assert_eq!(result, 7);
    }

    #[test]
    fn static_add_negative() {
        let result = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_add(-10, 5)
        };
        assert_eq!(result, -5);
    }

    #[test]
    fn static_add_zero() {
        let result = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_add(0, 0)
        };
        assert_eq!(result, 0);
    }

    #[test]
    fn static_add_wraps_on_overflow() {
        let result = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_add(i32::MAX, 1)
        };
        assert_eq!(result, i32::MIN);
    }
}

mod fixture_async_ref_self {
    use super::*;

    #[test]
    fn async_get_id_returns_future() {
        let handle =
            unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(123) };
        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_get_id(handle)
        };
        assert!(!future.is_null());
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_id_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_get_id_completes_with_value() {
        let handle =
            unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(123) };
        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_get_id(handle)
        };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<i32>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete(future) };
        assert_eq!(result, Ok(123));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_id_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_compute_sum_completes_with_sum() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 5)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 15)
        };

        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_compute_sum(handle)
        };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<i32>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete(future) };
        assert_eq!(result, Ok(20));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_compute_sum_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_async_ref_mut_self {
    use super::*;

    #[test]
    fn async_set_id_modifies_state() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_set_id(handle, 999)
        };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<()>(future, noop, 0) };
        let _ = unsafe { rustfuture::rust_future_complete::<()>(future) };
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_id_free(
                future,
            )
        };

        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            999
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_add_value_returns_new_count() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}

        let f1 = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_add_value(
                handle, 10,
            )
        };
        unsafe { rustfuture::rust_future_poll::<i32>(f1, noop, 0) };
        let r1 = unsafe { rustfuture::rust_future_complete(f1) };
        assert_eq!(r1, Ok(1));
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_add_value_free(
                f1,
            )
        };

        let f2 = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_add_value(
                handle, 20,
            )
        };
        unsafe { rustfuture::rust_future_poll::<i32>(f2, noop, 0) };
        let r2 = unsafe { rustfuture::rust_future_complete(f2) };
        assert_eq!(r2, Ok(2));
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_add_value_free(
                f2,
            )
        };

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_add_value_accumulates_sum() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}

        for i in 1..=5 {
            let future = unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_add_value(
                    handle,
                    i * 10,
                )
            };
            unsafe { rustfuture::rust_future_poll::<i32>(future, noop, 0) };
            let _ = unsafe { rustfuture::rust_future_complete::<i32>(future) };
            unsafe {
                boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_add_value_free(
                    future,
                )
            };
        }

        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(handle)
            },
            150
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_multiple_instances {
    use super::*;

    #[test]
    fn instances_have_independent_state() {
        let h1 =
            unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(100) };
        let h2 =
            unsafe { boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_id(200) };

        unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_id(h1, 111) };

        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(h1) },
            111
        );
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(h2) },
            200
        );

        unsafe {
            boltffi_release_class_boltffi_tests_classes_class_test_fixture(h1);
            boltffi_release_class_boltffi_tests_classes_class_test_fixture(h2);
        }
    }

    #[test]
    fn instances_have_independent_values() {
        let h1 = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let h2 = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(h1, 10) };
        unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(h1, 20) };
        unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(h2, 100) };

        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(h1)
            },
            2
        );
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_values_count(h2)
            },
            1
        );
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(h1)
            },
            30
        );
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_compute_sum(h2)
            },
            100
        );

        unsafe {
            boltffi_release_class_boltffi_tests_classes_class_test_fixture(h1);
            boltffi_release_class_boltffi_tests_classes_class_test_fixture(h2);
        }
    }
}

mod fixture_wire_encoded_returns {
    use super::*;

    #[test]
    fn try_get_value_ok_decodes_correctly() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 42)
        };

        let result = try_get_fixture_value(handle, 0);
        assert_eq!(result, Ok(42));

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn try_get_value_out_of_bounds_decodes_to_err() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        let result = try_get_fixture_value(handle, 0);
        assert!(result.is_err());

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn try_get_value_negative_index_decodes_to_err() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 42)
        };

        let result = try_get_fixture_value(handle, -1);
        assert!(result.is_err());

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn static_maybe_value_some_decodes_correctly() {
        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_maybe_value(true)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, Some(42));
    }

    #[test]
    fn static_maybe_value_none_decodes_correctly() {
        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_maybe_value(false)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, None);
    }
}

mod fixture_wire_encoded_constructors {
    use super::*;

    #[test]
    fn new_with_name_accepts_string() {
        let name = "test_name";
        let handle = new_fixture_with_name(name);
        assert_ne!(handle, 0);

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        let result: String = decode_buf(&buf);
        assert_eq!(result, "test_name");

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_name_empty_string() {
        let name = "";
        let handle = new_fixture_with_name(name);
        assert_ne!(handle, 0);

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        let result: String = decode_buf(&buf);
        assert_eq!(result, "");

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_point_accepts_record() {
        let point = FixturePoint { x: 1.5, y: 2.5 };
        let handle = new_fixture_with_point(point);
        assert_ne!(handle, 0);

        let result = get_fixture_point(handle);
        assert_eq!(result.x, 1.5);
        assert_eq!(result.y, 2.5);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_status_accepts_enum() {
        let handle = unsafe {
            boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_status(
                FixtureStatus::Active as i32,
            )
        };
        assert_ne!(handle, 0);

        let raw = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(handle)
        };
        let result: FixtureStatus = unsafe { std::mem::transmute(raw) };
        assert_eq!(result, FixtureStatus::Active);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn new_with_status_all_variants() {
        [
            FixtureStatus::Pending,
            FixtureStatus::Active,
            FixtureStatus::Completed,
            FixtureStatus::Failed,
        ]
        .iter()
        .for_each(|&status| {
            let handle = unsafe {
                boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_status(
                    status as i32,
                )
            };

            let raw = unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(handle)
            };
            let result: FixtureStatus = unsafe { std::mem::transmute(raw) };
            assert_eq!(result, status);

            unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
        });
    }

    #[test]
    fn new_full_accepts_all_params() {
        let name = "full_test";
        let point = FixturePoint { x: 3.0, y: 4.0 };

        let handle = new_full_fixture(42, name, point, FixtureStatus::Completed);
        assert_ne!(handle, 0);

        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_id(handle) },
            42
        );

        let name_buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        assert_eq!(decode_buf::<String>(&name_buf), "full_test");

        let result_point = get_fixture_point(handle);
        assert_eq!(result_point.x, 3.0);
        assert_eq!(result_point.y, 4.0);

        let raw_status = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(handle)
        };
        let result_status: FixtureStatus = unsafe { std::mem::transmute(raw_status) };
        assert_eq!(result_status, FixtureStatus::Completed);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_wire_encoded_getters {
    use super::*;

    #[test]
    fn get_name_returns_string() {
        let name = "getter_test";
        let handle = new_fixture_with_name(name);

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        let result: String = decode_buf(&buf);
        assert_eq!(result, "getter_test");

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn get_point_returns_record() {
        let point = FixturePoint { x: 10.0, y: 20.0 };
        let handle = new_fixture_with_point(point);

        let result = get_fixture_point(handle);
        assert_eq!(result.x, 10.0);
        assert_eq!(result.y, 20.0);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn get_status_returns_enum() {
        let handle = unsafe {
            boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_with_status(
                FixtureStatus::Failed as i32,
            )
        };

        let raw = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(handle)
        };
        let result: FixtureStatus = unsafe { std::mem::transmute(raw) };
        assert_eq!(result, FixtureStatus::Failed);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn get_values_empty_returns_empty_vec() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_values(handle)
        };
        let result: Vec<i32> = decode_i32_vec(buf);
        assert!(result.is_empty());

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn get_values_returns_vec() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 20)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 30)
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_values(handle)
        };
        let result: Vec<i32> = decode_i32_vec(buf);
        assert_eq!(result, vec![10, 20, 30]);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn get_optional_none() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_optional(handle)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, None);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn get_optional_some() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let opt = Some(99);
        let encoded = encode(&opt);
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_optional(
                handle,
                encoded.as_ptr(),
                encoded.len(),
            )
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_optional(handle)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, Some(99));

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn find_value_found() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 20)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 30)
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_find_value(handle, 20)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, Some(1));

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn find_value_not_found() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_find_value(handle, 999)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, None);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn values_near_point_filters_by_threshold() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 1)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 5)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, -3)
        };

        let point = FixturePoint { x: 3.0, y: 2.0 };
        let result = values_near_point(handle, point);
        assert_eq!(result, vec![1, 5, -3]);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn values_near_point_empty_values() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        let point = FixturePoint { x: 10.0, y: 10.0 };
        let result = values_near_point(handle, point);
        assert!(result.is_empty());

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_wire_encoded_setters {
    use super::*;

    #[test]
    fn set_name_accepts_string() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let name = "new_name";
        set_fixture_name(handle, name);

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        let result: String = decode_buf(&buf);
        assert_eq!(result, "new_name");

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_name_unicode() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let name = "こんにちは";
        set_fixture_name(handle, name);

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        let result: String = decode_buf(&buf);
        assert_eq!(result, "こんにちは");

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_point_accepts_record() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let point = FixturePoint { x: 5.5, y: 6.6 };
        set_fixture_point(handle, point);

        let result = get_fixture_point(handle);
        assert_eq!(result.x, 5.5);
        assert_eq!(result.y, 6.6);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_status_accepts_enum() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_status(
                handle,
                FixtureStatus::Completed as i32,
            )
        };

        let raw = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_status(handle)
        };
        let result: FixtureStatus = unsafe { std::mem::transmute(raw) };
        assert_eq!(result, FixtureStatus::Completed);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_values_accepts_vec() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let values: Vec<i32> = vec![100, 200, 300];
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_values(
                handle,
                values.as_ptr(),
                values.len(),
            )
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_values(handle)
        };
        let result: Vec<i32> = decode_i32_vec(buf);
        assert_eq!(result, vec![100, 200, 300]);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_values_empty_vec() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };

        let values: Vec<i32> = vec![];
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_values(
                handle,
                values.as_ptr(),
                values.len(),
            )
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_values(handle)
        };
        let result: Vec<i32> = decode_i32_vec(buf);
        assert!(result.is_empty());

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_optional_some() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let opt = Some(42);
        let encoded = encode(&opt);
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_optional(
                handle,
                encoded.as_ptr(),
                encoded.len(),
            )
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_optional(handle)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, Some(42));

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn set_optional_none() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let opt = Some(99);
        let encoded = encode(&opt);
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_optional(
                handle,
                encoded.as_ptr(),
                encoded.len(),
            )
        };

        let none: Option<i32> = None;
        let none_encoded = encode(&none);
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_set_optional(
                handle,
                none_encoded.as_ptr(),
                none_encoded.len(),
            )
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_optional(handle)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, None);

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fixture_wire_encoded_static {
    use super::*;

    #[test]
    fn static_concat_strings() {
        let a = "hello";
        let b = "world";
        let result = concat_strings(a, b);
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn static_concat_empty_strings() {
        let a = "";
        let b = "";
        let result = concat_strings(a, b);
        assert_eq!(result, "");
    }

    #[test]
    fn static_concat_one_empty() {
        let a = "foo";
        let b = "";
        let result = concat_strings(a, b);
        assert_eq!(result, "foo");
    }

    #[test]
    fn static_make_point_returns_record() {
        let result = make_static_point(7.0, 8.0);
        assert_eq!(result.x, 7.0);
        assert_eq!(result.y, 8.0);
    }

    #[test]
    fn static_identity_status_all_variants() {
        [
            FixtureStatus::Pending,
            FixtureStatus::Active,
            FixtureStatus::Completed,
            FixtureStatus::Failed,
        ]
        .iter()
        .for_each(|&status| {
            let raw = unsafe {
                boltffi_method_class_boltffi_tests_classes_class_test_fixture_static_identity_status(
                    status as i32,
                )
            };
            let result: FixtureStatus = unsafe { std::mem::transmute(raw) };
            assert_eq!(result, status);
        });
    }

    #[test]
    fn static_try_parse_ok() {
        let s = "123";
        let result = try_parse_fixture(s);
        assert_eq!(result, Ok(123));
    }

    #[test]
    fn static_try_parse_negative() {
        let s = "-456";
        let result = try_parse_fixture(s);
        assert_eq!(result, Ok(-456));
    }

    #[test]
    fn static_try_parse_err() {
        let s = "not_a_number";
        let result = try_parse_fixture(s);
        assert!(result.is_err());
    }
}

mod fixture_wire_encoded_async {
    use super::*;

    #[test]
    fn async_get_name_returns_string() {
        let name = "async_test";
        let handle = new_fixture_with_name(name);

        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_get_name(handle)
        };
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<String>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete(future) };
        assert_eq!(result, Ok("async_test".to_string()));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_get_name_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_set_name_modifies_state() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        let name = "async_name";

        let future = async_set_fixture_name(handle, name);
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<()>(future, noop, 0) };
        let _ = unsafe { rustfuture::rust_future_complete::<()>(future) };
        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_set_name_free(
                future,
            )
        };

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_get_name(handle)
        };
        let result: String = decode_buf(&buf);
        assert_eq!(result, "async_name");

        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_find_found() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 20)
        };

        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_find(handle, 20)
        };
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<Option<i32>>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete::<Option<i32>>(future) };
        assert_eq!(result, Ok(Some(1)));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_find_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_find_not_found() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 10)
        };

        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_find(handle, 999)
        };
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<Option<i32>>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete::<Option<i32>>(future) };
        assert_eq!(result, Ok(None));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_find_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_try_get_ok() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();
        unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_add_value(handle, 77)
        };

        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_try_get(handle, 0)
        };
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<Result<i32, String>>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete::<Result<i32, String>>(future) };
        assert_eq!(result, Ok(Ok(77)));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_try_get_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }

    #[test]
    fn async_try_get_err() {
        let handle = boltffi_init_class_boltffi_tests_classes_class_test_fixture_new_default();

        let future = unsafe {
            boltffi_method_class_boltffi_tests_classes_class_test_fixture_async_try_get(handle, 99)
        };
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<Result<i32, String>>(future, noop, 0) };

        let result = unsafe { rustfuture::rust_future_complete::<Result<i32, String>>(future) };
        assert!(matches!(result, Ok(Err(_))));

        unsafe {
            boltffi_async_method_class_boltffi_tests_classes_class_test_fixture_async_try_get_free(
                future,
            )
        };
        unsafe { boltffi_release_class_boltffi_tests_classes_class_test_fixture(handle) };
    }
}

mod fallible_service_ffi {
    use super::*;

    #[test]
    fn new_returns_valid_handle() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }

    #[test]
    fn get_value_ok_mode_returns_doubled_key() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();
        unsafe {
            boltffi_method_class_boltffi_tests_results_fallible_service_set_failure_mode(handle, 0)
        };

        let result = get_fallible_service_value(handle, 5);
        assert_eq!(result, Ok(10));

        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }

    #[test]
    fn get_value_failure_mode_1_returns_not_found() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();
        unsafe {
            boltffi_method_class_boltffi_tests_results_fallible_service_set_failure_mode(handle, 1)
        };

        let result = get_fallible_service_value(handle, 5);
        assert_eq!(result, Err(FixtureError::NotFound));

        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }

    #[test]
    fn get_value_failure_mode_2_returns_invalid_input() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();
        unsafe {
            boltffi_method_class_boltffi_tests_results_fallible_service_set_failure_mode(handle, 2)
        };

        let result = get_fallible_service_value(handle, 5);
        assert_eq!(result, Err(FixtureError::InvalidInput));

        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }

    #[test]
    fn get_optional_positive_returns_some() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_results_fallible_service_get_optional(handle, 5)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, Some(15));

        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }

    #[test]
    fn get_optional_zero_returns_none() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_results_fallible_service_get_optional(handle, 0)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, None);

        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }

    #[test]
    fn get_optional_negative_returns_none() {
        let handle = boltffi_init_class_boltffi_tests_results_fallible_service_new();

        let buf = unsafe {
            boltffi_method_class_boltffi_tests_results_fallible_service_get_optional(handle, -5)
        };
        let result: Option<i32> = decode_buf(&buf);
        assert_eq!(result, None);

        unsafe { boltffi_release_class_boltffi_tests_results_fallible_service(handle) };
    }
}

mod cancellable_task_ffi {
    use super::*;

    #[test]
    fn new_returns_valid_handle() {
        let handle = boltffi_init_class_boltffi_tests_results_cancellable_task_new();
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_results_cancellable_task(handle) };
    }

    #[test]
    fn was_started_initially_false() {
        let handle = boltffi_init_class_boltffi_tests_results_cancellable_task_new();
        assert!(!unsafe {
            boltffi_method_class_boltffi_tests_results_cancellable_task_was_started(handle)
        });
        unsafe { boltffi_release_class_boltffi_tests_results_cancellable_task(handle) };
    }

    #[test]
    fn was_completed_initially_false() {
        let handle = boltffi_init_class_boltffi_tests_results_cancellable_task_new();
        assert!(!unsafe {
            boltffi_method_class_boltffi_tests_results_cancellable_task_was_completed(handle)
        });
        unsafe { boltffi_release_class_boltffi_tests_results_cancellable_task(handle) };
    }

    #[test]
    fn instant_task_sets_started_and_completed() {
        let handle = boltffi_init_class_boltffi_tests_results_cancellable_task_new();

        let future = unsafe {
            boltffi_method_class_boltffi_tests_results_cancellable_task_instant_task(handle)
        };
        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        unsafe { rustfuture::rust_future_poll::<i32>(future, noop, 0) };
        let result = unsafe { rustfuture::rust_future_complete(future) };
        assert_eq!(result, Ok(99));
        unsafe {
            boltffi_async_method_class_boltffi_tests_results_cancellable_task_instant_task_free(
                future,
            )
        };

        assert!(unsafe {
            boltffi_method_class_boltffi_tests_results_cancellable_task_was_started(handle)
        });
        assert!(unsafe {
            boltffi_method_class_boltffi_tests_results_cancellable_task_was_completed(handle)
        });

        unsafe { boltffi_release_class_boltffi_tests_results_cancellable_task(handle) };
    }
}

mod sync_processor_ffi {
    use super::*;

    #[test]
    fn new_returns_valid_handle() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_callbacks_sync_processor_new(5) };
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_callbacks_sync_processor(handle) };
    }

    #[test]
    fn multiple_instances_independent() {
        let h1 = unsafe { boltffi_init_class_boltffi_tests_callbacks_sync_processor_new(2) };
        let h2 = unsafe { boltffi_init_class_boltffi_tests_callbacks_sync_processor_new(10) };
        assert_ne!(h1, 0);
        assert_ne!(h2, 0);
        assert_ne!(h1, h2);
        unsafe {
            boltffi_release_class_boltffi_tests_callbacks_sync_processor(h1);
            boltffi_release_class_boltffi_tests_callbacks_sync_processor(h2);
        }
    }
}

mod async_processor_ffi {
    use super::*;

    #[test]
    fn new_returns_valid_handle() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_callbacks_async_processor_new(100) };
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_callbacks_async_processor(handle) };
    }

    #[test]
    fn multiple_instances_independent() {
        let h1 = unsafe { boltffi_init_class_boltffi_tests_callbacks_async_processor_new(50) };
        let h2 = unsafe { boltffi_init_class_boltffi_tests_callbacks_async_processor_new(200) };
        assert_ne!(h1, 0);
        assert_ne!(h2, 0);
        assert_ne!(h1, h2);
        unsafe {
            boltffi_release_class_boltffi_tests_callbacks_async_processor(h1);
            boltffi_release_class_boltffi_tests_callbacks_async_processor(h2);
        }
    }
}

mod thread_safe_counter_ffi {
    use super::*;
    use std::thread;

    struct SendHandle(u64);
    impl Clone for SendHandle {
        fn clone(&self) -> Self {
            *self
        }
    }
    impl Copy for SendHandle {}
    impl SendHandle {
        fn get(self) -> u64 {
            self.0
        }
    }

    #[test]
    fn new_returns_valid_handle() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(0) };
        assert_ne!(handle, 0);
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn get_returns_initial_value() {
        let handle =
            unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(42) };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(handle) },
            42
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn set_modifies_value() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(0) };
        unsafe { boltffi_method_class_boltffi_tests_classes_thread_safe_counter_set(handle, 100) };
        assert_eq!(
            unsafe { boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(handle) },
            100
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn add_returns_new_value() {
        let handle =
            unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(10) };
        let result = unsafe {
            boltffi_method_class_boltffi_tests_classes_thread_safe_counter_add(handle, 5)
        };
        assert_eq!(result, 15);
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn increment_returns_new_value() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(0) };
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_thread_safe_counter_increment(handle)
            },
            1
        );
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_thread_safe_counter_increment(handle)
            },
            2
        );
        assert_eq!(
            unsafe {
                boltffi_method_class_boltffi_tests_classes_thread_safe_counter_increment(handle)
            },
            3
        );
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn concurrent_increments_are_safe() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(0) };
        let handle_ptr = SendHandle(handle);

        let threads: Vec<_> = (0..10)
            .map(|_| {
                thread::spawn(move || {
                    for _ in 0..1000 {
                        unsafe {
                            boltffi_method_class_boltffi_tests_classes_thread_safe_counter_increment(
                                handle_ptr.get(),
                            )
                        };
                    }
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        let final_value =
            unsafe { boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(handle) };
        assert_eq!(final_value, 10_000);
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn concurrent_adds_are_safe() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(0) };
        let handle_ptr = SendHandle(handle);

        let threads: Vec<_> = (0..4)
            .map(|i| {
                thread::spawn(move || {
                    for _ in 0..250 {
                        unsafe {
                            boltffi_method_class_boltffi_tests_classes_thread_safe_counter_add(
                                handle_ptr.get(),
                                i + 1,
                            )
                        };
                    }
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        let final_value =
            unsafe { boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(handle) };
        assert_eq!(final_value, 250 * (1 + 2 + 3 + 4));
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }

    #[test]
    fn concurrent_reads_and_writes_are_safe() {
        let handle = unsafe { boltffi_init_class_boltffi_tests_classes_thread_safe_counter_new(0) };
        let handle_ptr = SendHandle(handle);

        let writers: Vec<_> = (0..4)
            .map(|_| {
                thread::spawn(move || {
                    for _ in 0..500 {
                        unsafe {
                            boltffi_method_class_boltffi_tests_classes_thread_safe_counter_increment(
                                handle_ptr.get(),
                            )
                        };
                    }
                })
            })
            .collect();

        let readers: Vec<_> = (0..4)
            .map(|_| {
                thread::spawn(move || {
                    let mut last = 0;
                    for _ in 0..500 {
                        let current = unsafe {
                            boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(
                                handle_ptr.get(),
                            )
                        };
                        assert!(current >= last);
                        last = current;
                    }
                })
            })
            .collect();

        for t in writers {
            t.join().unwrap();
        }
        for t in readers {
            t.join().unwrap();
        }

        let final_value =
            unsafe { boltffi_method_class_boltffi_tests_classes_thread_safe_counter_get(handle) };
        assert_eq!(final_value, 4 * 500);
        unsafe { boltffi_release_class_boltffi_tests_classes_thread_safe_counter(handle) };
    }
}
