//! Source-shaped views for generated `Java_*` native methods.
//!
//! A native method contract is typed for correctness, but a C template needs a
//! linear method body: JNI parameter declarations, borrowed array locals,
//! direct-record writeback variables, status checks, C bridge call arguments,
//! cleanup blocks, and the final return expression.
//!
//! This module performs only that projection. It does not decide what a
//! parameter means or how a return crosses the boundary. Those facts come from
//! `contract::parameter` and `contract::return_value`; this module prepares the
//! exact fields the Askama method template prints.

mod array;
mod direct_buffer;
mod parameter;
mod record;
mod view;

pub use array::BorrowedArrayParameterView;
pub use direct_buffer::DirectBufferParameterView;
pub use parameter::NativeParameterView;
pub use record::RecordParameterView;
pub use view::NativeMethodView;
