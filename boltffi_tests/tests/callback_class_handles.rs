#![cfg(not(miri))]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use boltffi::__private::{BoxFromCallbackHandle, CallbackHandle};
use boltffi_tests::*;

unsafe extern "C" {
    fn boltffi_tests_create_class_receiver(
        vtable: *const ClassHandleReceiverVTable,
        identity: u64,
    ) -> CallbackHandle;
    fn boltffi_tests_consume_callback_class(handle: u64) -> u32;
}

#[derive(Default)]
struct Delivery {
    class_handle: AtomicU64,
    callback_drops: AtomicUsize,
}

impl Delivery {
    extern "C" fn release(identity: u64) {
        let delivery = unsafe { Arc::from_raw(identity as *const Self) };
        delivery.callback_drops.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn retain(identity: u64) -> u64 {
        unsafe { Arc::increment_strong_count(identity as *const Self) };
        identity
    }

    extern "C" fn attach(identity: u64, handle: u64, callback: u32) {
        let delivery = unsafe { &*(identity as *const Self) };
        assert_eq!(callback, 42);
        delivery.class_handle.store(handle, Ordering::SeqCst);
    }
}

#[test]
fn callback_only_class_remains_owned_after_delivery_on_another_thread() {
    static VTABLE: ClassHandleReceiverVTable = ClassHandleReceiverVTable {
        free: Delivery::release,
        clone: Delivery::retain,
        attach: Delivery::attach,
    };
    let delivery = Arc::new(Delivery::default());
    let drops = Arc::new(AtomicUsize::new(0));
    let class = CallbackOnlyHandle::with_drops(Arc::clone(&drops));
    let identity = Arc::into_raw(Arc::clone(&delivery)) as u64;
    std::thread::spawn(move || {
        let handle = unsafe { boltffi_tests_create_class_receiver(&VTABLE, identity) };
        let receiver = unsafe { ForeignClassHandleReceiver::box_from_callback_handle(handle) };
        receiver.attach(class, 42);
    })
    .join()
    .unwrap();

    assert_eq!(delivery.callback_drops.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let handle = delivery.class_handle.load(Ordering::SeqCst);
    assert_ne!(handle, 0);
    assert_eq!(unsafe { boltffi_tests_consume_callback_class(handle) }, 42);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
