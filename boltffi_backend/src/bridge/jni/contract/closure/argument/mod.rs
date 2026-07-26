//! Arguments passed when Rust invokes a JVM-owned closure.
//!
//! A JVM-owned closure is called by Rust through a generated C trampoline. The
//! trampoline receives C ABI parameters, prepares the Java values required by
//! the JVM closure method, and converts the result back to the C return shape.
//!
//! This module owns the argument side of that contract. It keeps the C
//! parameters, JVM call expressions, and setup requirements together for
//! scalars, encoded bytes, direct vectors, and nested closure handles.

mod bytes;
mod c_abi;
mod c_bridge;
mod direct_vector;
mod handle;
mod jvm;
mod parameters;
mod record;
mod scalar;
mod setup;
mod success_out;

pub use bytes::ClosureBytesArgument;
pub use c_abi::ClosureCParameter;
pub use direct_vector::ClosureDirectVectorArgument;
pub use handle::ClosureHandleArgument;
pub(crate) use record::ClosureRecordArgument;
pub use scalar::ClosureScalarArgument;
pub use success_out::ClosureSuccessOutArgument;

/// One inline-closure argument crossing the JNI bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureArgument {
    kind: ClosureArgumentKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClosureArgumentKind {
    Scalar(ClosureScalarArgument),
    Bytes(ClosureBytesArgument),
    DirectVector(ClosureDirectVectorArgument),
    Record(ClosureRecordArgument),
    Closure(ClosureHandleArgument),
    SuccessOut(ClosureSuccessOutArgument),
}
