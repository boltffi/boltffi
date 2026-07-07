use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;

use boltffi::__private::{FfiBuf, FfiStatus, rustfuture::RustFuturePoll};
use boltffi_core::wire::{WireDecode, WireEncode};
use boltffi_tests::*;

fn decode_buf<T: WireDecode>(buf: FfiBuf) -> T {
    let (value, _) = T::decode_from(unsafe { buf.as_byte_slice() }).unwrap();
    value
}

fn decode_direct_or_wire_vec<T: WireDecode + WireEncode>(buf: FfiBuf) -> Vec<T> {
    if buf.align() == core::mem::align_of::<T>() {
        unsafe { buf.into_vec::<T>() }
    } else {
        decode_buf(buf)
    }
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

fn decode_result<T, E: WireDecode>(error: FfiBuf, value: T) -> Result<T, E> {
    if error.is_empty() {
        Ok(value)
    } else {
        Err(decode_buf(error))
    }
}

fn assert_ok(status: FfiStatus) {
    assert_eq!(status, FfiStatus::OK);
}

mod primitives {
    use super::*;

    #[test]
    fn scalar_widths_cross_directly() {
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_i8(3, 4) },
            7
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_u8(3, 4) },
            7
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_i16(30, 40) },
            70
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_u16(30, 40) },
            70
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_i32(300, 400) },
            700
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_u32(300, 400) },
            700
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_i64(3000, 4000) },
            7000
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_u64(3000, 4000) },
            7000
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_isize(5, 6) },
            11
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_add_usize(5, 6) },
            11
        );
    }

    #[test]
    fn direct_ref_mut_and_void_crossings_run() {
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_mix_floats(2.0, 3.5) },
            9.0
        );
        assert!(unsafe { boltffi_function_boltffi_tests_primitives_toggle(false) });
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_primitives_read_ref(42) },
            42
        );
        assert_ok(unsafe { boltffi_function_boltffi_tests_primitives_bump_in_place(9) });
        assert_ok(boltffi_function_boltffi_tests_primitives_noop());
    }
}

mod bytes {
    use super::*;

    #[test]
    fn byte_inputs_and_returns_cross_as_bytes() {
        let data = vec![1u8, 2, 3, 4];
        assert_eq!(
            with_encoded(&data, |ptr, len| unsafe {
                boltffi_function_boltffi_tests_bytes_byte_sum(ptr, len)
            }),
            10
        );
        assert_eq!(
            with_encoded(&data, |ptr, len| unsafe {
                boltffi_function_boltffi_tests_bytes_borrowed_byte_sum(ptr, len)
            }),
            10
        );
        let echoed = with_encoded(&data, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_bytes_echo_bytes(ptr, len)
        });
        assert_eq!(decode_buf::<Vec<u8>>(echoed), data);
    }

    #[test]
    fn mutable_byte_slice_writes_into_current_buffer() {
        let mut input = vec![0u8; 6];
        let written = unsafe {
            boltffi_function_boltffi_tests_bytes_fill_bytes(input.as_mut_ptr(), input.len())
        };
        assert_eq!(written, 6);
        assert_eq!(input, vec![1, 4, 7, 10, 13, 16]);
    }
}

mod strings {
    use super::*;

    #[test]
    fn string_value_ref_and_mut_crossings_use_wire_bytes() {
        let shouted = with_encoded(&"hello".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_strings_shout(ptr, len)
        });
        assert_eq!(decode_buf::<String>(shouted), "HELLO");

        let borrowed_len = with_encoded(&"hello".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_strings_borrowed_len(ptr, len)
        });
        assert_eq!(borrowed_len, 5);

        let mut out = FfiBuf::empty();
        with_encoded_pair(
            &"base".to_string(),
            &":suffix".to_string(),
            |text_ptr, text_len, suffix_ptr, suffix_len| {
                assert_ok(unsafe {
                    boltffi_function_boltffi_tests_strings_rewrite(
                        text_ptr as *mut u8,
                        text_len,
                        &mut out,
                        suffix_ptr,
                        suffix_len,
                    )
                });
            },
        );
        assert_eq!(decode_buf::<String>(out), "base:suffix");
    }
}

mod records {
    use super::*;

    #[test]
    fn direct_record_functions_keep_c_layout_values() {
        let mut rect =
            unsafe { boltffi_function_boltffi_tests_records_direct_make_rect(1.0, 2.0, 3.0, 4.0) };
        assert_eq!(
            rect,
            FixtureRect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            }
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_records_direct_rect_area(rect) },
            12.0
        );
        assert_eq!(
            unsafe { boltffi_function_boltffi_tests_records_direct_rect_x(&rect) },
            1.0
        );
        assert_ok(unsafe {
            boltffi_function_boltffi_tests_records_direct_scale_rect_in_place(&mut rect, 2.0)
        });
        assert_eq!(rect.width, 6.0);
        assert_eq!(rect.height, 8.0);
    }

    #[test]
    fn encoded_record_functions_roundtrip_wire_values() {
        let record = FixtureMessageRecord {
            label: "old".to_string(),
            anchor: FixturePoint { x: 2.0, y: 3.0 },
            status: FixtureStatus::Completed,
        };
        let description = with_encoded(&record, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_records_encoded_describe_message(ptr, len)
        });
        assert_eq!(decode_buf::<String>(description), "old:2:3:Completed");

        let label_len = with_encoded(&record, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_records_encoded_peek_label(ptr, len)
        });
        assert_eq!(label_len, 3);

        let mut out = FfiBuf::empty();
        with_encoded_pair(
            &record,
            &"new".to_string(),
            |record_ptr, record_len, label_ptr, label_len| {
                assert_ok(unsafe {
                    boltffi_function_boltffi_tests_records_encoded_relabel(
                        record_ptr as *mut u8,
                        record_len,
                        &mut out,
                        label_ptr,
                        label_len,
                    )
                });
            },
        );
        assert_eq!(
            decode_buf::<FixtureMessageRecord>(out),
            FixtureMessageRecord {
                label: "new".to_string(),
                anchor: FixturePoint { x: 2.0, y: 3.0 },
                status: FixtureStatus::Completed,
            }
        );
    }

    #[test]
    fn encoded_record_constructor_returns_expected_record() {
        let message = with_encoded(&"made".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_records_encoded_make_message(ptr, len)
        });
        assert_eq!(
            decode_buf::<FixtureMessageRecord>(message),
            FixtureMessageRecord {
                label: "made".to_string(),
                anchor: FixturePoint { x: 5.0, y: 8.0 },
                status: FixtureStatus::Active,
            }
        );
    }
}

mod enums_and_options {
    use super::*;

    #[test]
    fn c_style_enum_crosses_as_its_repr() {
        assert_eq!(
            unsafe {
                boltffi_function_boltffi_tests_enums_next_status(FixtureStatus::Pending as i32)
            },
            FixtureStatus::Active as i32
        );
    }

    #[test]
    fn data_enum_crosses_through_wire() {
        let shape = FixtureShape::Rect {
            width: 3.0,
            height: 4.0,
        };
        let area = with_encoded(&shape, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_enums_area(ptr, len)
        });
        assert_eq!(area, 12.0);

        let widened = with_encoded(&FixtureShape::Line(5.0), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_enums_widen(ptr, len, 2.0)
        });
        assert_eq!(decode_buf::<FixtureShape>(widened), FixtureShape::Line(7.0));
    }

    #[test]
    fn options_cover_scalar_encoded_and_direct_record_shapes() {
        assert_eq!(
            decode_buf::<Option<i32>>(unsafe {
                boltffi_function_boltffi_tests_options_simple_maybe_double(4)
            }),
            Some(8)
        );
        let doubled = with_encoded(&Some(5), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_options_maybe_double(ptr, len)
        });
        assert_eq!(decode_buf::<Option<i32>>(doubled), Some(10));

        let scaled = with_encoded(&Some(4.0), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_options_maybe_scale(ptr, len)
        });
        assert_eq!(decode_buf::<Option<f64>>(scaled), Some(6.0));

        assert_eq!(
            decode_buf::<Option<FixturePoint>>(unsafe {
                boltffi_function_boltffi_tests_options_maybe_point(true)
            }),
            Some(FixturePoint { x: 2.0, y: 3.0 })
        );

        let rect = FixtureRect {
            x: 9.0,
            y: 8.0,
            width: 7.0,
            height: 6.0,
        };
        let direct = with_encoded(&Some(rect), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_options_point_or_origin(ptr, len)
        });
        assert_eq!(direct, rect);

        let label = with_encoded(&Some("tag".to_string()), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_options_maybe_label(ptr, len)
        });
        assert_eq!(
            decode_buf::<Option<String>>(label),
            Some("tag:seen".to_string())
        );
    }
}

mod vectors_and_collections {
    use super::*;

    #[test]
    fn direct_and_encoded_vectors_cross_correctly() {
        let values = [1u32, 2, 3, 4];
        assert_eq!(
            unsafe {
                boltffi_function_boltffi_tests_vectors_sum_u32(values.as_ptr(), values.len())
            },
            10
        );

        let floats = [2.0f64, 4.0, 6.0];
        let halved = unsafe {
            boltffi_function_boltffi_tests_vectors_halve_f64(floats.as_ptr(), floats.len())
        };
        assert_eq!(
            decode_direct_or_wire_vec::<f64>(halved),
            vec![1.0, 2.0, 3.0]
        );

        let rects = [
            FixtureRect {
                x: 1.0,
                y: 1.0,
                width: 2.0,
                height: 2.0,
            },
            FixtureRect {
                x: -1.0,
                y: 0.0,
                width: 1.0,
                height: 5.0,
            },
        ];
        let bounds = unsafe {
            boltffi_function_boltffi_tests_vectors_bounding_box(
                rects.as_ptr().cast::<u8>(),
                core::mem::size_of_val(&rects),
            )
        };
        assert_eq!(
            bounds,
            FixtureRect {
                x: -1.0,
                y: 0.0,
                width: 4.0,
                height: 5.0,
            }
        );

        let joined = with_encoded(&vec!["a".to_string(), "b".to_string()], |ptr, len| unsafe {
            boltffi_function_boltffi_tests_vectors_join_labels(ptr, len)
        });
        assert_eq!(decode_buf::<String>(joined), "a|b");

        let split = with_encoded(&"a|b|c".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_vectors_split_labels(ptr, len)
        });
        assert_eq!(
            decode_buf::<Vec<String>>(split),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );

        let statuses = unsafe { boltffi_function_boltffi_tests_vectors_statuses(4) };
        assert_eq!(
            decode_direct_or_wire_vec::<i32>(statuses),
            vec![
                FixtureStatus::Pending as i32,
                FixtureStatus::Active as i32,
                FixtureStatus::Completed as i32,
                FixtureStatus::Failed as i32,
            ]
        );
    }

    #[test]
    fn maps_tuples_and_nested_options_cross_through_wire() {
        let labels = HashMap::from([("one".to_string(), 1), ("two".to_string(), 2)]);
        let total = with_encoded(&labels, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_collections_tally(ptr, len)
        });
        assert_eq!(total, 3);

        let inverted = with_encoded(&labels, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_collections_invert(ptr, len)
        });
        assert_eq!(
            decode_buf::<HashMap<String, i32>>(inverted),
            HashMap::from([("eno".to_string(), -1), ("owt".to_string(), -2)])
        );

        let pair = with_encoded(&"label".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_collections_pair_up(7, ptr, len)
        });
        assert_eq!(
            decode_buf::<(i32, String)>(pair),
            (14, "label:7".to_string())
        );

        let nested = vec![Some("ab".to_string()), None, Some("cde".to_string())];
        let length = with_encoded(&nested, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_collections_deep(ptr, len)
        });
        assert_eq!(length, 5);
    }
}

mod custom_types {
    use super::*;

    #[test]
    fn custom_type_values_options_and_vectors_use_the_declared_repr() {
        let shifted = with_encoded(&1000_i64, |ptr, len| unsafe {
            boltffi_function_boltffi_tests_customs_shift_instant(ptr, len, 250)
        });
        assert_eq!(decode_buf::<i64>(shifted), 1250);

        assert_eq!(
            decode_buf::<Option<i64>>(unsafe {
                boltffi_function_boltffi_tests_customs_maybe_instant(true)
            }),
            Some(1234)
        );

        assert_eq!(
            decode_buf::<Vec<i64>>(unsafe { boltffi_function_boltffi_tests_customs_instants(3) }),
            vec![0, 1000, 2000]
        );
    }
}

mod asynchronous {
    use super::*;

    extern "C" fn noop(_: u64, _: RustFuturePoll) {}

    #[test]
    fn async_direct_completion_returns_value() {
        let future = unsafe { boltffi_function_boltffi_tests_asynchronous_async_add(20, 22) };
        unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_add_poll(future, 0, noop)
        };
        let mut status = FfiStatus::OK;
        let value = unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_add_complete(
                future,
                &mut status,
            )
        };
        assert_ok(status);
        assert_eq!(value, 42);
        unsafe { boltffi_async_function_boltffi_tests_asynchronous_async_add_free(future) };
    }

    #[test]
    fn async_encoded_completion_returns_value() {
        let future = with_encoded(&"Ali".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_asynchronous_async_greet(ptr, len)
        });
        unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_greet_poll(future, 0, noop)
        };
        let mut status = FfiStatus::OK;
        let value = unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_greet_complete(
                future,
                &mut status,
            )
        };
        assert_ok(status);
        assert_eq!(decode_buf::<String>(value), "hello Ali");
        unsafe { boltffi_async_function_boltffi_tests_asynchronous_async_greet_free(future) };
    }

    #[test]
    fn async_direct_record_completion_returns_value() {
        let future =
            unsafe { boltffi_function_boltffi_tests_asynchronous_async_make_rect(2.0, -3.0) };
        unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_make_rect_poll(future, 0, noop)
        };
        let mut status = FfiStatus::OK;
        let rect = unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_make_rect_complete(
                future,
                &mut status,
            )
        };
        assert_ok(status);
        assert_eq!(
            rect,
            FixtureRect {
                x: 2.0,
                y: -3.0,
                width: 3.0,
                height: 4.0,
            }
        );
        unsafe { boltffi_async_function_boltffi_tests_asynchronous_async_make_rect_free(future) };
    }

    #[test]
    fn async_void_completion_sets_ok_status() {
        let future = boltffi_function_boltffi_tests_asynchronous_async_ping();
        unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_ping_poll(future, 0, noop)
        };
        let mut status = FfiStatus::INTERNAL_ERROR;
        unsafe {
            boltffi_async_function_boltffi_tests_asynchronous_async_ping_complete(
                future,
                &mut status,
            )
        };
        assert_ok(status);
        unsafe { boltffi_async_function_boltffi_tests_asynchronous_async_ping_free(future) };
    }
}

mod closures {
    use super::*;

    unsafe extern "C" fn add_three(_: *mut c_void, value: u32) -> u32 {
        value + 3
    }

    unsafe extern "C" fn uppercase(_: *mut c_void, ptr: *const u8, len: usize) -> FfiBuf {
        let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
        let text = decode_buf::<String>(FfiBuf::from_vec(bytes.to_vec()));
        FfiBuf::wire_encode(&text.to_uppercase())
    }

    unsafe extern "C" fn release(_: *mut c_void) {}

    #[test]
    fn closure_parameters_call_back_into_foreign_functions() {
        assert_eq!(
            unsafe {
                boltffi_function_boltffi_tests_closures_apply(
                    add_three,
                    ptr::null_mut(),
                    release,
                    10,
                )
            },
            23
        );
        assert_eq!(
            unsafe {
                boltffi_function_boltffi_tests_closures_apply_boxed(
                    add_three,
                    ptr::null_mut(),
                    release,
                    10,
                )
            },
            26
        );
        assert_eq!(
            unsafe {
                boltffi_function_boltffi_tests_closures_apply_optional(
                    Some(add_three),
                    ptr::null_mut(),
                    Some(release),
                    10,
                )
            },
            13
        );
        assert_eq!(
            unsafe {
                boltffi_function_boltffi_tests_closures_apply_optional(
                    None,
                    ptr::null_mut(),
                    None,
                    10,
                )
            },
            10
        );
    }

    #[test]
    fn encoded_closure_parameters_move_wire_values_both_directions() {
        let result = with_encoded(&"hello".to_string(), |ptr, len| unsafe {
            boltffi_function_boltffi_tests_closures_map_label(
                uppercase,
                ptr::null_mut(),
                release,
                ptr,
                len,
            )
        });
        assert_eq!(decode_buf::<String>(result), "HELLO:IN");
    }
}

mod results {
    use super::*;

    #[test]
    fn fallible_direct_and_encoded_success_values_use_out_pointers() {
        let mut divided = 0;
        let divide_error =
            unsafe { boltffi_function_boltffi_tests_results_try_divide(12, 3, &mut divided) };
        assert_eq!(decode_result::<_, String>(divide_error, divided), Ok(4));

        let mut rect = FixtureRect::default();
        let rect_error =
            unsafe { boltffi_function_boltffi_tests_results_try_rect(false, &mut rect) };
        assert_eq!(
            decode_result::<_, String>(rect_error, rect),
            Ok(FixtureRect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            })
        );

        let mut message = FfiBuf::empty();
        let message_error =
            unsafe { boltffi_function_boltffi_tests_results_try_message(false, &mut message) };
        assert_eq!(
            decode_result::<_, String>(message_error, decode_buf::<FixtureMessageRecord>(message)),
            Ok(FixtureMessageRecord {
                label: "ok".to_string(),
                anchor: FixturePoint { x: 1.0, y: 1.5 },
                status: FixtureStatus::Completed,
            })
        );
    }

    #[test]
    fn fallible_errors_return_encoded_error_buffers() {
        let mut direct = 0;
        let divide_error =
            unsafe { boltffi_function_boltffi_tests_results_try_divide(12, 0, &mut direct) };
        assert_eq!(
            decode_result::<_, String>(divide_error, direct),
            Err("divide by zero".to_string())
        );

        let mut status_value = 0;
        let status_error =
            unsafe { boltffi_function_boltffi_tests_results_try_status_err(-1, &mut status_value) };
        assert_eq!(
            decode_result::<_, FixtureStatus>(status_error, status_value),
            Err(FixtureStatus::Failed)
        );

        let mut shape_value = 0;
        let shape_error =
            unsafe { boltffi_function_boltffi_tests_results_try_shape_err(-5, &mut shape_value) };
        assert_eq!(
            decode_result::<_, FixtureShape>(shape_error, shape_value),
            Err(FixtureShape::Line(5.0))
        );
    }
}
