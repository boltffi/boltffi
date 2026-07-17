//! A callback: the foreign side implements the trait behind a repr(C) vtable, and an
//! exported function dispatches into it through `dyn Logger`.

use pim_macros::{callback, export};

#[callback]
pub trait Logger {
    fn log(&self, line: String) -> bool;
    fn level(&self) -> u32;
}

#[export]
pub fn drain_logs(logger: Box<dyn Logger>, lines: Vec<String>) -> u32 {
    let level = logger.level();
    let mut kept = 0;
    for line in lines {
        if logger.log(format!("[{level}] {line}")) {
            kept += 1;
        }
    }
    kept
}
