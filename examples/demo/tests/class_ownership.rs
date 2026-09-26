#![cfg(feature = "csharp-demo")]

use std::ptr;

use boltffi::__private::{FfiBuf, RustFutureHandle};
use demo as _;

unsafe extern "C" {
    fn boltffi_init_class_demo_classes_ownership_message_drops_new() -> u64;
    fn boltffi_init_class_demo_classes_ownership_owned_message_new(
        text: *const u8,
        length: usize,
        drops: u64,
    ) -> u64;
    fn boltffi_method_class_demo_classes_ownership_message_drops_count(handle: u64) -> u32;
    fn boltffi_release_class_demo_classes_ownership_message_drops(handle: u64);
    fn boltffi_release_class_demo_classes_ownership_owned_message(handle: u64);
    fn boltffi_function_demo_classes_ownership_consume_message(message: u64) -> u32;
    fn boltffi_function_demo_classes_ownership_consume_messages(first: u64, second: u64) -> u32;
    fn boltffi_function_demo_classes_ownership_consume_messages_async(
        first: u64,
        second: u64,
    ) -> RustFutureHandle;
    fn boltffi_async_function_demo_classes_ownership_consume_messages_async_free(
        future: RustFutureHandle,
    );
    fn boltffi_function_demo_classes_ownership_join_message(
        labels: *const u8,
        length: usize,
        message: u64,
    ) -> FfiBuf;
    fn boltffi_method_class_demo_classes_ownership_owned_message_combine(
        receiver: u64,
        other: u64,
    ) -> u32;
    fn boltffi_method_class_demo_classes_ownership_message_store_set(
        receiver: u64,
        message: u64,
        carrier: *const u8,
        length: usize,
    ) -> RustFutureHandle;
    fn boltffi_async_method_class_demo_classes_ownership_message_store_set_free(
        future: RustFutureHandle,
    );
    fn boltffi_function_demo_classes_ownership_hold_message(message: u64) -> RustFutureHandle;
    fn boltffi_async_function_demo_classes_ownership_hold_message_cancel(future: RustFutureHandle);
    fn boltffi_async_function_demo_classes_ownership_hold_message_free(future: RustFutureHandle);
    fn boltffi_function_demo_classes_ownership_hold_owned_message(message: u64)
    -> RustFutureHandle;
    fn boltffi_async_function_demo_classes_ownership_hold_owned_message_cancel(
        future: RustFutureHandle,
    );
    fn boltffi_async_function_demo_classes_ownership_hold_owned_message_free(
        future: RustFutureHandle,
    );
}

struct MessageLifetime {
    handle: u64,
}

impl MessageLifetime {
    fn new() -> Self {
        Self {
            handle: unsafe { boltffi_init_class_demo_classes_ownership_message_drops_new() },
        }
    }

    fn message(&self) -> u64 {
        let text = FfiBuf::wire_encode(&"message".to_owned());
        unsafe {
            boltffi_init_class_demo_classes_ownership_owned_message_new(
                text.as_ptr(),
                text.len(),
                self.handle,
            )
        }
    }

    fn dropped(&self) -> u32 {
        unsafe { boltffi_method_class_demo_classes_ownership_message_drops_count(self.handle) }
    }
}

impl Drop for MessageLifetime {
    fn drop(&mut self) {
        unsafe { boltffi_release_class_demo_classes_ownership_message_drops(self.handle) };
    }
}

#[test]
fn null_first_argument_releases_later_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let result = unsafe { boltffi_function_demo_classes_ownership_consume_messages(0, message) };

    assert_eq!(result, 0);
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn decoding_failure_releases_later_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let result =
        unsafe { boltffi_function_demo_classes_ownership_join_message(ptr::null(), 0, message) };

    assert!(result.is_empty());
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn async_null_first_argument_releases_later_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let future =
        unsafe { boltffi_function_demo_classes_ownership_consume_messages_async(0, message) };
    unsafe { boltffi_async_function_demo_classes_ownership_consume_messages_async_free(future) };

    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn null_receiver_releases_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let result =
        unsafe { boltffi_method_class_demo_classes_ownership_owned_message_combine(0, message) };

    assert_eq!(result, 0);
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn async_null_receiver_releases_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let future = unsafe {
        boltffi_method_class_demo_classes_ownership_message_store_set(0, message, ptr::null(), 0)
    };
    unsafe { boltffi_async_method_class_demo_classes_ownership_message_store_set_free(future) };

    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn rejected_transfer_releases_ownership_after_the_last_borrow() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_demo_classes_ownership_hold_message(message) };

    let result = unsafe { boltffi_function_demo_classes_ownership_consume_message(message) };

    assert_eq!(result, 0);
    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_demo_classes_ownership_hold_message_cancel(future);
        boltffi_async_function_demo_classes_ownership_hold_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn cancelling_an_owned_argument_drops_it_once() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_demo_classes_ownership_hold_owned_message(message) };

    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_demo_classes_ownership_hold_owned_message_cancel(future);
        boltffi_async_function_demo_classes_ownership_hold_owned_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn disposing_a_borrowed_argument_defers_its_drop() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_demo_classes_ownership_hold_message(message) };

    unsafe { boltffi_release_class_demo_classes_ownership_owned_message(message) };

    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_demo_classes_ownership_hold_message_cancel(future);
        boltffi_async_function_demo_classes_ownership_hold_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
}
