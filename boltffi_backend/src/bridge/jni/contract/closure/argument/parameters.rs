//! Flat C parameter list for generated closure trampolines.
//!
//! Closure arguments are stored by meaning, but the generated C function pointer
//! needs a plain parameter list. A scalar contributes one parameter, bytes and
//! direct vectors contribute two, and nested closures contribute their native
//! call/context/release group.
//!
//! This module exposes that flattened ABI list from the closure argument
//! contract. It keeps signature printing away from the logic that prepares Java
//! arguments.

use super::{ClosureArgument, ClosureArgumentKind, ClosureCParameter};

impl ClosureArgument {
    /// Returns the C parameters accepted by the closure call trampoline.
    pub fn c_parameters(&self) -> Vec<ClosureCParameter> {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.c_parameters(),
            ClosureArgumentKind::Bytes(argument) => argument.c_parameters(),
            ClosureArgumentKind::DirectVector(argument) => argument.c_parameters(),
            ClosureArgumentKind::Record(argument) => argument.c_parameters(),
            ClosureArgumentKind::Closure(argument) => argument.c_parameters(),
            ClosureArgumentKind::SuccessOut(argument) => argument.c_parameters(),
        }
    }

    /// Returns the C parameters accepted by the Rust-owned closure handle entrypoint.
    pub fn handle_parameters(&self) -> Vec<ClosureCParameter> {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.handle_parameters(),
            ClosureArgumentKind::Bytes(argument) => argument.handle_parameters(),
            ClosureArgumentKind::DirectVector(argument) => argument.handle_parameters(),
            ClosureArgumentKind::Record(argument) => argument.handle_parameters(),
            ClosureArgumentKind::Closure(argument) => argument.handle_parameters(),
            ClosureArgumentKind::SuccessOut(argument) => argument.handle_parameters(),
        }
    }
}
