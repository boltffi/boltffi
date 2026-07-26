//! Local setup required before invoking a JVM-owned closure.
//!
//! Some closure arguments require work before the static JVM method can be
//! called. Encoded bytes need byte arrays, direct vectors need primitive arrays,
//! and nested closures need handle tokens with call and release functions.
//!
//! This module exposes those setup groups from the closure argument contract so
//! templates do not match argument variants just to find local work.

use super::{
    ClosureArgument, ClosureArgumentKind, ClosureBytesArgument, ClosureDirectVectorArgument,
    ClosureHandleArgument, ClosureRecordArgument,
};

use crate::bridge::jni::SuccessOutArgument;

impl ClosureArgument {
    /// Returns the success out argument when this closure argument carries one.
    pub fn success_out(&self) -> Option<SuccessOutArgument> {
        match &self.kind {
            ClosureArgumentKind::SuccessOut(argument) => Some(argument.argument().clone()),
            ClosureArgumentKind::Scalar(_)
            | ClosureArgumentKind::Bytes(_)
            | ClosureArgumentKind::DirectVector(_)
            | ClosureArgumentKind::Record(_)
            | ClosureArgumentKind::Closure(_) => None,
        }
    }

    /// Returns the byte-array argument when the JVM receives encoded bytes.
    pub fn call_bytes(&self) -> Option<&ClosureBytesArgument> {
        match &self.kind {
            ClosureArgumentKind::Scalar(_) => None,
            ClosureArgumentKind::Bytes(argument) => Some(argument),
            ClosureArgumentKind::DirectVector(_) => None,
            ClosureArgumentKind::Record(_) => None,
            ClosureArgumentKind::Closure(_) => None,
            ClosureArgumentKind::SuccessOut(_) => None,
        }
    }

    /// Returns the byte-array argument when the JVM sends encoded bytes.
    pub fn handle_bytes(&self) -> Option<&ClosureBytesArgument> {
        self.call_bytes()
    }

    /// Returns the direct-vector argument when the JVM receives an array.
    pub fn call_direct_vector(&self) -> Option<&ClosureDirectVectorArgument> {
        match &self.kind {
            ClosureArgumentKind::DirectVector(argument) => Some(argument),
            ClosureArgumentKind::Scalar(_)
            | ClosureArgumentKind::Bytes(_)
            | ClosureArgumentKind::Record(_)
            | ClosureArgumentKind::Closure(_)
            | ClosureArgumentKind::SuccessOut(_) => None,
        }
    }

    /// Returns the closure-handle argument when the JVM receives a nested closure.
    pub fn call_closure(&self) -> Option<&ClosureHandleArgument> {
        match &self.kind {
            ClosureArgumentKind::Closure(argument) => Some(argument),
            ClosureArgumentKind::Scalar(_)
            | ClosureArgumentKind::Bytes(_)
            | ClosureArgumentKind::DirectVector(_)
            | ClosureArgumentKind::Record(_)
            | ClosureArgumentKind::SuccessOut(_) => None,
        }
    }

    pub(crate) fn call_record(&self) -> Option<&ClosureRecordArgument> {
        match &self.kind {
            ClosureArgumentKind::Record(argument) => Some(argument),
            ClosureArgumentKind::Scalar(_)
            | ClosureArgumentKind::Bytes(_)
            | ClosureArgumentKind::DirectVector(_)
            | ClosureArgumentKind::Closure(_)
            | ClosureArgumentKind::SuccessOut(_) => None,
        }
    }

    pub(crate) fn handle_record(&self) -> Option<&ClosureRecordArgument> {
        self.call_record()
    }

    /// Returns the direct-vector argument when the JVM sends an array.
    pub fn handle_direct_vector(&self) -> Option<&ClosureDirectVectorArgument> {
        self.call_direct_vector()
    }
}
