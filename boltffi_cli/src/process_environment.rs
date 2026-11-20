//! The lock tests hold around the process environment. Tests run as threads of one process,
//! so a test that sets environment variables and a test whose cargo run inherits them must
//! not overlap.

use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Holds the process environment until the guard drops, also after an earlier holder
/// panicked, so one failing test does not fail every test after it.
pub fn hold() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}
