use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct PendingHandle {
    cancelled: Arc<AtomicBool>,
}

impl PendingHandle {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        CancellationToken {
            cancelled: Arc::clone(&self.cancelled),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for PendingHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn boltffi_pending_cancel(handle: *mut PendingHandle) {
    if !handle.is_null() {
        unsafe { (*handle).cancel() };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn boltffi_pending_free(handle: *mut PendingHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

pub type AsyncCallback<T> = extern "C" fn(user_data: *mut core::ffi::c_void, status: crate::FfiStatus, result: T);
pub type AsyncCallbackVoid = extern "C" fn(user_data: *mut core::ffi::c_void, status: crate::FfiStatus);
pub type AsyncCallbackString = extern "C" fn(user_data: *mut core::ffi::c_void, status: crate::FfiStatus, result: crate::FfiString);
