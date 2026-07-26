mod read;
mod runtime;
mod size;
mod value;
mod write;

use super::primitive::Primitive;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SequenceElement {
    General,
    Fixed(u64),
    Primitive(Primitive),
    String,
}

pub use read::Reader;
pub use runtime::Runtime;
pub use size::Sizer;
pub use value::ValueMemberAccess;
pub use write::{EncodedWrite, WireBuffer, WriteStatement, Writer};
