use std::cell::RefCell;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FfiStatus {
    pub code: i32,
}

impl FfiStatus {
    pub const OK: Self = Self { code: 0 };
    pub const NULL_POINTER: Self = Self { code: 1 };
    pub const BUFFER_TOO_SMALL: Self = Self { code: 2 };
    pub const INVALID_ARG: Self = Self { code: 3 };
    pub const CANCELLED: Self = Self { code: 4 };
    pub const WRONG_THREAD: Self = Self { code: 5 };
    pub const INTERNAL_ERROR: Self = Self { code: 100 };

    pub const fn new(code: i32) -> Self {
        Self { code }
    }

    pub const fn is_ok(self) -> bool {
        self.code == 0
    }

    pub const fn is_err(self) -> bool {
        self.code != 0
    }
}

impl Default for FfiStatus {
    fn default() -> Self {
        Self::OK
    }
}

impl From<i32> for FfiStatus {
    fn from(code: i32) -> Self {
        Self { code }
    }
}

impl From<FfiStatus> for i32 {
    fn from(status: FfiStatus) -> Self {
        status.code
    }
}

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Records the message the next `boltffi_last_error` call will return.
///
/// Cold and never inlined on purpose. Without that, LLVM pulls the whole body
/// (thread-local access, the `RefCell` borrow guard, and a `String`
/// allocation) into every generated export shim, which on the demo crate cost
/// ~71 KiB of wasm across ~300 shims.
#[cold]
#[inline(never)]
pub fn set_last_error(message: impl Into<String>) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(message.into());
    });
}

/// Records a parameter-decoding failure that carries no error value.
///
/// These helpers exist so the generated shims call one outlined function
/// instead of inlining a `format!` per failure site. Keeping them `#[cold]`
/// and taking `&dyn` receivers also collapses what used to be one
/// monomorphisation per error type down to a single instantiation.
#[cold]
#[inline(never)]
pub fn set_last_error_len(parameter: &str, problem: &str, buf_len: usize) {
    set_last_error(format!("{parameter}: {problem} (buf_len={buf_len})"));
}

/// Records a parameter-decoding failure whose cause implements [`Display`].
#[cold]
#[inline(never)]
pub fn set_last_error_display(
    parameter: &str,
    problem: &str,
    error: &dyn core::fmt::Display,
    buf_len: usize,
) {
    set_last_error(format!(
        "{parameter}: {problem}: {error} (buf_len={buf_len})"
    ));
}

/// Records a parameter-decoding failure whose cause only implements [`Debug`].
#[cold]
#[inline(never)]
pub fn set_last_error_debug(
    parameter: &str,
    problem: &str,
    error: &dyn core::fmt::Debug,
    buf_len: usize,
) {
    set_last_error(format!(
        "{parameter}: {problem}: {error:?} (buf_len={buf_len})"
    ));
}

pub fn take_last_error() -> Option<String> {
    LAST_ERROR.with(|cell| cell.borrow_mut().take())
}

pub fn clear_last_error() {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = None;
    });
}
