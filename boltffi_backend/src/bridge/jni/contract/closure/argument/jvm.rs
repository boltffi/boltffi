//! JVM call shape for closure arguments.
//!
//! A generated closure trampoline receives C ABI parameters and calls a static
//! JVM method. The closure argument contract already knows which C parameters
//! form bytes, direct vectors, nested closures, and scalar values.
//!
//! This module projects those typed arguments into the C expressions passed to
//! the JVM method. It does not decide argument meaning; it only spells the Java
//! call from the existing contract.

use crate::bridge::c::{ArgumentList, Expression};

use super::{ClosureArgument, ClosureArgumentKind};

impl ClosureArgument {
    /// Returns the expressions passed to the static JVM closure method.
    pub fn jvm_arguments(&self) -> Vec<Expression> {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.jvm_arguments(),
            ClosureArgumentKind::Bytes(argument) => argument.jvm_arguments(),
            ClosureArgumentKind::DirectVector(argument) => argument.jvm_arguments(),
            ClosureArgumentKind::Record(argument) => argument.jvm_arguments(),
            ClosureArgumentKind::Closure(argument) => argument.jvm_arguments(),
            ClosureArgumentKind::SuccessOut(argument) => argument.jvm_arguments(),
        }
    }

    /// Returns the argument list passed to the static JVM closure method.
    pub fn jvm_argument_list(arguments: &[Self]) -> ArgumentList {
        ArgumentList::from_iter(arguments.iter().flat_map(ClosureArgument::jvm_arguments))
    }

    /// Returns the expressions passed into the Rust closure call function.
    pub fn rust_arguments(&self) -> Vec<Expression> {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.rust_arguments(),
            ClosureArgumentKind::Bytes(argument) => argument.rust_arguments(),
            ClosureArgumentKind::DirectVector(argument) => argument.rust_arguments(),
            ClosureArgumentKind::Record(argument) => argument.rust_arguments(),
            ClosureArgumentKind::Closure(argument) => argument.rust_arguments(),
            ClosureArgumentKind::SuccessOut(argument) => argument.rust_arguments(),
        }
    }

    /// Returns the argument list passed to the Rust closure call function.
    pub fn rust_argument_list(arguments: &[Self]) -> ArgumentList {
        ArgumentList::from_iter(arguments.iter().flat_map(ClosureArgument::rust_arguments))
    }

    /// Returns the JNI method descriptor segment for this argument.
    pub fn jni_signature(&self) -> &'static str {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.jni_signature(),
            ClosureArgumentKind::Bytes(argument) => argument.jni_signature(),
            ClosureArgumentKind::DirectVector(argument) => argument.jni_signature(),
            ClosureArgumentKind::Record(argument) => argument.jni_signature(),
            ClosureArgumentKind::Closure(argument) => argument.jni_signature(),
            ClosureArgumentKind::SuccessOut(argument) => argument.jni_signature(),
        }
    }
}
