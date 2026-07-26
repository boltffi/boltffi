//! JVM names and JNI symbol spellings.
//!
//! One generated class has several valid spellings. Java source uses dotted
//! package names, class lookup uses slash-separated paths, generated files need a
//! stable path, and native functions use JNI's escaped `Java_*` symbol form.
//! Keeping those as loose strings would let each caller apply a slightly
//! different rule.
//!
//! This module validates JVM name segments once and exposes the specific
//! spellings needed by the bridge contract and templates. Code outside this
//! module asks for a class path or a JNI symbol; it does not split packages,
//! uppercase fragments, or hand-roll underscore escaping.

mod class_path;
mod lookup_text;
mod modified_utf8;
mod segment;
mod symbol;

pub use class_path::JvmClassPath;
pub use lookup_text::LookupText;
pub use modified_utf8::ModifiedUtf8;
pub use segment::JvmNameSegment;
pub use symbol::JniSymbolName;
