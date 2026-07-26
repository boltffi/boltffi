//! Source fields for arguments passed through inline closure trampolines.
//!
//! A registered closure signature is used in two directions. Rust can call a
//! JVM-owned closure through a C trampoline, and Java can call a Rust-owned
//! closure handle through a generated native method. Both paths need the same
//! argument contract, but they print different source fields.
//!
//! This module splits those printable fields by argument family: byte arrays,
//! direct primitive vectors, nested closure handles, and the C parameters that
//! form the trampoline signature. It does not classify the closure signature
//! again.

mod bytes;
mod c_parameter;
mod direct_vector;
mod handle;
mod record;

pub use bytes::ClosureBytesArgumentView;
pub use c_parameter::ClosureCParameterView;
pub use direct_vector::ClosureDirectVectorArgumentView;
pub use handle::ClosureHandleArgumentView;
pub use record::ClosureRecordArgumentView;
