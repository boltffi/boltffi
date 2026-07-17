//! Per-invocation metadata capture: each `#[data]`/`#[export]` expansion emits its own
//! link-section record, built in const context so type references resolve through rustc.

mod cross;
mod record;
mod type_info;

pub use cross::{FfiCross, note_panic};
pub use record::{
    SOURCE_RECORD_MAGIC, SOURCE_SECTION_MACH_O, SOURCE_SECTION_MACH_O_NAME, SOURCE_SECTION_OBJECT,
    record, record_len,
};
pub use type_info::{DESC_CAPACITY, DescBuf, TypeDesc, TypeInfo};
