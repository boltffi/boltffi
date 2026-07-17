//! Per-invocation metadata capture: each `#[data]`/`#[export]` expansion emits its own
//! link-section record, built in const context so type references resolve through rustc.

mod record;

pub use record::{
    SOURCE_RECORD_MAGIC, SOURCE_SECTION_MACH_O, SOURCE_SECTION_MACH_O_NAME, SOURCE_SECTION_OBJECT,
    record, record_len,
};
