use std::cell::RefCell;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FfiStatus {
    pub code: i32,
}

impl FfiStatus {
    pub const BUFFER_TOO_SMALL: Self = Self { code: 2 };
    pub const CANCELLED: Self = Self { code: 4 };
    pub const INTERNAL_ERROR: Self = Self { code: 100 };
    pub const INVALID_ARG: Self = Self { code: 3 };
    pub const NULL_POINTER: Self = Self { code: 1 };
    pub const OK: Self = Self { code: 0 };

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

pub fn set_last_error(message: impl Into<String>) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(message.into());
    });
}

pub fn take_last_error() -> Option<String> {
    LAST_ERROR.with(|cell| cell.borrow_mut().take())
}

pub fn clear_last_error() {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = None;
    });
}
