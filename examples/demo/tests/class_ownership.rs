#![cfg(feature = "csharp-demo")]

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

use boltffi::__private::{FfiBuf, RustFutureHandle};
use demo as _;

unsafe extern "C" {
    fn boltffi_init_class_4demo12MessageDrops3new() -> u64;
    fn boltffi_init_class_4demo12OwnedMessage3new(
        text: *const u8,
        length: usize,
        drops: u64,
    ) -> u64;
    fn boltffi_method_class_4demo12MessageDrops5count(handle: u64) -> u32;
    fn boltffi_release_class_4demo12MessageDrops(handle: u64);
    fn boltffi_release_class_4demo12OwnedMessage(handle: u64);
    fn boltffi_function_4demo15consume_message(message: u64) -> u32;
    fn boltffi_function_4demo29consume_message_with_callback(
        callback: unsafe extern "C" fn(*mut c_void, u32) -> u32,
        context: *mut c_void,
        release: unsafe extern "C" fn(*mut c_void),
        message: u64,
    ) -> u32;
    fn boltffi_function_4demo31consume_message_before_callback(
        message: u64,
        callback: unsafe extern "C" fn(*mut c_void, u32) -> u32,
        context: *mut c_void,
        release: unsafe extern "C" fn(*mut c_void),
    ) -> u32;
    fn boltffi_function_4demo16consume_messages(first: u64, second: u64) -> u32;
    fn boltffi_function_4demo22consume_messages_async(
        first: u64,
        second: u64,
    ) -> RustFutureHandle;
    fn boltffi_async_function_4demo22consume_messages_async_free(
        future: RustFutureHandle,
    );
    fn boltffi_function_4demo12join_message(
        labels: *const u8,
        length: usize,
        message: u64,
    ) -> FfiBuf;
    fn boltffi_method_class_4demo12OwnedMessage7combine(
        receiver: u64,
        other: u64,
    ) -> u32;
    fn boltffi_method_class_4demo12MessageStore3set(
        receiver: u64,
        message: u64,
        carrier: *const u8,
        length: usize,
    ) -> RustFutureHandle;
    fn boltffi_async_method_class_4demo12MessageStore3set_free(
        future: RustFutureHandle,
    );
    fn boltffi_function_4demo12hold_message(message: u64) -> RustFutureHandle;
    fn boltffi_async_function_4demo12hold_message_cancel(future: RustFutureHandle);
    fn boltffi_async_function_4demo12hold_message_free(future: RustFutureHandle);
    fn boltffi_function_4demo18hold_owned_message(message: u64)
    -> RustFutureHandle;
    fn boltffi_async_function_4demo18hold_owned_message_cancel(
        future: RustFutureHandle,
    );
    fn boltffi_async_function_4demo18hold_owned_message_free(
        future: RustFutureHandle,
    );
}

struct MessageLifetime {
    handle: u64,
}

impl MessageLifetime {
    fn new() -> Self {
        Self {
            handle: unsafe { boltffi_init_class_4demo12MessageDrops3new() },
        }
    }

    fn message(&self) -> u64 {
        let text = FfiBuf::wire_encode(&"message".to_owned());
        unsafe {
            boltffi_init_class_4demo12OwnedMessage3new(
                text.as_ptr(),
                text.len(),
                self.handle,
            )
        }
    }

    fn dropped(&self) -> u32 {
        unsafe { boltffi_method_class_4demo12MessageDrops5count(self.handle) }
    }
}

impl Drop for MessageLifetime {
    fn drop(&mut self) {
        unsafe { boltffi_release_class_4demo12MessageDrops(self.handle) };
    }
}

#[derive(Default)]
struct ClosureCapture {
    calls: AtomicU32,
    releases: AtomicU32,
}

impl ClosureCapture {
    fn context(&mut self) -> *mut c_void {
        ptr::from_mut(self).cast()
    }

    unsafe extern "C" fn invoke(context: *mut c_void, value: u32) -> u32 {
        let capture = unsafe { &*context.cast::<Self>() };
        capture.calls.fetch_add(1, Ordering::SeqCst);
        value
    }

    unsafe extern "C" fn release(context: *mut c_void) {
        let capture = unsafe { &*context.cast::<Self>() };
        capture.releases.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn invalid_class_releases_closures_on_either_side_of_the_argument() {
    let mut capture = ClosureCapture::default();
    let first_result = unsafe {
        boltffi_function_4demo29consume_message_with_callback(
            ClosureCapture::invoke,
            capture.context(),
            ClosureCapture::release,
            0,
        )
    };
    let second_result = unsafe {
        boltffi_function_4demo31consume_message_before_callback(
            0,
            ClosureCapture::invoke,
            capture.context(),
            ClosureCapture::release,
        )
    };

    assert_eq!((first_result, second_result), (0, 0));
    assert_eq!(capture.calls.load(Ordering::SeqCst), 0);
    assert_eq!(capture.releases.load(Ordering::SeqCst), 2);
}

#[test]
fn rejected_transfer_releases_its_closure_before_the_borrow_ends() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_4demo12hold_message(message) };
    let mut capture = ClosureCapture::default();

    let result = unsafe {
        boltffi_function_4demo31consume_message_before_callback(
            message,
            ClosureCapture::invoke,
            capture.context(),
            ClosureCapture::release,
        )
    };

    assert_eq!(result, 0);
    assert_eq!(capture.calls.load(Ordering::SeqCst), 0);
    assert_eq!(capture.releases.load(Ordering::SeqCst), 1);
    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_4demo12hold_message_cancel(future);
        boltffi_async_function_4demo12hold_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
    assert_eq!(capture.releases.load(Ordering::SeqCst), 1);
}

#[test]
fn null_first_argument_releases_later_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let result = unsafe { boltffi_function_4demo16consume_messages(0, message) };

    assert_eq!(result, 0);
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn decoding_failure_releases_later_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let result =
        unsafe { boltffi_function_4demo12join_message(ptr::null(), 0, message) };

    assert!(result.is_empty());
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn async_null_first_argument_releases_later_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let future =
        unsafe { boltffi_function_4demo22consume_messages_async(0, message) };
    unsafe { boltffi_async_function_4demo22consume_messages_async_free(future) };

    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn null_receiver_releases_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let result =
        unsafe { boltffi_method_class_4demo12OwnedMessage7combine(0, message) };

    assert_eq!(result, 0);
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn async_null_receiver_releases_owned_arguments() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();

    let future = unsafe {
        boltffi_method_class_4demo12MessageStore3set(0, message, ptr::null(), 0)
    };
    unsafe { boltffi_async_method_class_4demo12MessageStore3set_free(future) };

    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn rejected_transfer_releases_ownership_after_the_last_borrow() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_4demo12hold_message(message) };

    let result = unsafe { boltffi_function_4demo15consume_message(message) };

    assert_eq!(result, 0);
    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_4demo12hold_message_cancel(future);
        boltffi_async_function_4demo12hold_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn cancelling_an_owned_argument_drops_it_once() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_4demo18hold_owned_message(message) };

    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_4demo18hold_owned_message_cancel(future);
        boltffi_async_function_4demo18hold_owned_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
}

#[test]
fn disposing_a_borrowed_argument_defers_its_drop() {
    let lifetime = MessageLifetime::new();
    let message = lifetime.message();
    let future = unsafe { boltffi_function_4demo12hold_message(message) };

    unsafe { boltffi_release_class_4demo12OwnedMessage(message) };

    assert_eq!(lifetime.dropped(), 0);
    unsafe {
        boltffi_async_function_4demo12hold_message_cancel(future);
        boltffi_async_function_4demo12hold_message_free(future);
    }
    assert_eq!(lifetime.dropped(), 1);
}
