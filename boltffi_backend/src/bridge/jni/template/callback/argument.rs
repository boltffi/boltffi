//! Source fields for arguments passed from Rust into JVM callbacks.
//!
//! A callback vtable slot receives C ABI parameters from Rust, then calls a
//! static JVM method. Those parameters are already typed by the callback
//! contract, but the C template needs source-ready names: byte-array locals,
//! record arrays, callback handle tokens, closure handle tokens, completion
//! callbacks, and the flat C parameter declarations.
//!
//! This module is the projection between those two shapes. It does not decide
//! whether an argument is encoded, direct, borrowed, or async completion data.
//! It only prepares the fields that the callback templates print.

use crate::bridge::{
    c::{ArgumentList, Identifier, Statement},
    jni::{
        CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
        CallbackCompletionArgument, CallbackHandleArgument, CallbackRecordArgument,
    },
};

pub struct CallbackCParameterView {
    pub name: Identifier,
    pub declaration: Statement,
    pub class_release: Option<Identifier>,
}

pub struct CallbackBytesArgumentView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
}

pub struct CallbackRecordArgumentView {
    pub array: Identifier,
    pub parameter: Identifier,
}

pub struct CallbackHandleArgumentView {
    pub handle: Identifier,
    pub parameter: Identifier,
}

pub struct CallbackClosureArgumentView {
    pub handle: Identifier,
    pub call: Identifier,
    pub context: Identifier,
    pub release: Identifier,
    pub handle_new: Identifier,
    pub handle_release: Identifier,
}

pub struct CallbackCompletionArgumentView {
    pub callback: Identifier,
    pub failure_arguments: ArgumentList,
}

impl CallbackCParameterView {
    pub fn from_parameter(parameter: &CallbackCParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            declaration: parameter.declaration().clone(),
            class_release: parameter.class_release().cloned(),
        }
    }
}

impl CallbackBytesArgumentView {
    pub fn from_argument(argument: &CallbackBytesArgument) -> Self {
        Self {
            name: argument.name().clone(),
            pointer: argument.pointer().clone(),
            length: argument.length().clone(),
        }
    }
}

impl CallbackRecordArgumentView {
    pub fn from_argument(argument: &CallbackRecordArgument) -> Self {
        Self {
            array: argument.array().clone(),
            parameter: argument.parameter().clone(),
        }
    }
}

impl CallbackHandleArgumentView {
    pub fn from_argument(argument: &CallbackHandleArgument) -> Self {
        Self {
            handle: argument.handle().clone(),
            parameter: argument.parameter().clone(),
        }
    }
}

impl CallbackClosureArgumentView {
    pub fn from_argument(argument: &CallbackClosureArgument) -> Self {
        Self {
            handle: argument.handle().clone(),
            call: argument.call().clone(),
            context: argument.context().clone(),
            release: argument.release().clone(),
            handle_new: argument.handle_new().clone(),
            handle_release: argument.handle_release().clone(),
        }
    }
}

impl CallbackCompletionArgumentView {
    pub fn from_argument(argument: &CallbackCompletionArgument) -> Self {
        Self {
            callback: argument.callback().clone(),
            failure_arguments: argument.failure_arguments().clone(),
        }
    }
}
