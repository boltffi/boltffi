use crate::status::FfiStatus;
use std::panic::catch_unwind;

pub const PANIC_STATUS: FfiStatus = FfiStatus { code: 10 };

pub fn catch_ffi_panic<F>(operation: F) -> FfiStatus
where
    F: FnOnce() -> FfiStatus + std::panic::UnwindSafe,
{
    match catch_unwind(operation) {
        Ok(status) => status,
        Err(_) => PANIC_STATUS,
    }
}
