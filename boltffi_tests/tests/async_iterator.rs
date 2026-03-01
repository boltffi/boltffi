use boltffi::__private::{
    FfiBuf,
    rustfuture::{self, RustFuturePoll},
};
use boltffi_core::wire::WireDecode;
use boltffi_tests::*;

fn decode_option_i32(buf: &FfiBuf<u8>) -> Option<i32> {
    let (result, _) = <Option<i32>>::decode_from(unsafe { buf.as_slice() }).unwrap();
    result
}

/// Poll a future to completion synchronously (works for immediately-ready futures).
fn complete_option_i32(future: boltffi::__private::RustFutureHandle) -> Option<Option<i32>> {
    extern "C" fn noop(_: u64, _: RustFuturePoll) {}
    unsafe { rustfuture::rust_future_poll::<Option<i32>>(future, noop, 0) };
    unsafe { rustfuture::rust_future_complete::<Option<i32>>(future) }
}

mod number_iterator_new {
    use super::*;

    #[test]
    fn returns_non_null_handle() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_count_to_three(obj) };
        assert!(!iter.is_null());
        unsafe { boltffi_number_iterator_count_to_three_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }

    #[test]
    fn empty_returns_non_null_handle() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_empty(obj) };
        assert!(!iter.is_null());
        unsafe { boltffi_number_iterator_empty_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }
}

mod number_iterator_count_to_three {
    use super::*;

    #[test]
    fn next_returns_future_handle() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_count_to_three(obj) };

        let future = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
        assert!(!future.is_null());

        unsafe { boltffi_number_iterator_count_to_three_next_free(future) };
        unsafe { boltffi_number_iterator_count_to_three_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }

    #[test]
    fn yields_items_in_order() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_count_to_three(obj) };

        let f1 = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
        let r1 = complete_option_i32(f1);
        assert_eq!(r1, Some(Some(1)));
        unsafe { boltffi_number_iterator_count_to_three_next_free(f1) };

        let f2 = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
        let r2 = complete_option_i32(f2);
        assert_eq!(r2, Some(Some(2)));
        unsafe { boltffi_number_iterator_count_to_three_next_free(f2) };

        let f3 = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
        let r3 = complete_option_i32(f3);
        assert_eq!(r3, Some(Some(3)));
        unsafe { boltffi_number_iterator_count_to_three_next_free(f3) };

        unsafe { boltffi_number_iterator_count_to_three_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }

    #[test]
    fn returns_none_after_exhausted() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_count_to_three(obj) };

        for _ in 0..3 {
            let f = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
            complete_option_i32(f);
            unsafe { boltffi_number_iterator_count_to_three_next_free(f) };
        }

        // Fourth call must return None (end of stream)
        let f = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
        let result = complete_option_i32(f);
        assert_eq!(result, Some(None));
        unsafe { boltffi_number_iterator_count_to_three_next_free(f) };

        unsafe { boltffi_number_iterator_count_to_three_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }

    #[test]
    fn next_complete_returns_wire_encoded_option() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_count_to_three(obj) };

        extern "C" fn noop(_: u64, _: RustFuturePoll) {}
        let future = unsafe { boltffi_number_iterator_count_to_three_next(iter) };
        unsafe { rustfuture::rust_future_poll::<Option<i32>>(future, noop, 0) };

        let mut status = boltffi::__private::FfiStatus::OK;
        let buf: FfiBuf<u8> = unsafe { boltffi_number_iterator_count_to_three_next_complete(future, &mut status) };
        assert_eq!(status, boltffi::__private::FfiStatus::OK);

        let value = decode_option_i32(&buf);
        assert_eq!(value, Some(1));

        unsafe { boltffi_number_iterator_count_to_three_next_free(future) };
        unsafe { boltffi_number_iterator_count_to_three_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }

    #[test]
    fn multiple_independent_iterators_are_independent() {
        let obj = boltffi_number_iterator_new();

        let iter1 = unsafe { boltffi_number_iterator_count_to_three(obj) };
        let iter2 = unsafe { boltffi_number_iterator_count_to_three(obj) };

        // Advance iter1 by one
        let f1 = unsafe { boltffi_number_iterator_count_to_three_next(iter1) };
        let r1 = complete_option_i32(f1);
        assert_eq!(r1, Some(Some(1)));
        unsafe { boltffi_number_iterator_count_to_three_next_free(f1) };

        // iter2 should still start at 1
        let f2 = unsafe { boltffi_number_iterator_count_to_three_next(iter2) };
        let r2 = complete_option_i32(f2);
        assert_eq!(r2, Some(Some(1)));
        unsafe { boltffi_number_iterator_count_to_three_next_free(f2) };

        unsafe { boltffi_number_iterator_count_to_three_free(iter1) };
        unsafe { boltffi_number_iterator_count_to_three_free(iter2) };
        unsafe { boltffi_number_iterator_free(obj) };
    }
}

mod number_iterator_empty {
    use super::*;

    #[test]
    fn first_next_returns_none() {
        let obj = boltffi_number_iterator_new();
        let iter = unsafe { boltffi_number_iterator_empty(obj) };

        let f = unsafe { boltffi_number_iterator_empty_next(iter) };
        let result = complete_option_i32(f);
        assert_eq!(result, Some(None));
        unsafe { boltffi_number_iterator_empty_next_free(f) };

        unsafe { boltffi_number_iterator_empty_free(iter) };
        unsafe { boltffi_number_iterator_free(obj) };
    }
}
