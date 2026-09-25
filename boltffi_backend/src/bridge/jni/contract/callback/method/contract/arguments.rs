//! Grouped argument views for callback method rendering.
//!
//! A callback method stores its arguments as one ordered list, but the C source
//! template needs them in several groups. Byte arrays need allocation, direct
//! vectors need element copies, records need fixed-size byte arrays, handles
//! need wrapper construction, and async completions need their own token setup.
//!
//! This module exposes those groups from the method contract. It keeps template
//! code from matching every argument kind just to find the subset it needs.

use crate::bridge::{
    c::{ArgumentList, Expression, Identifier},
    jni::{
        CallbackArgument, CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
        CallbackCompletionArgument, CallbackDirectVectorArgument, CallbackHandleArgument,
        CallbackRecordArgument,
    },
};

use super::CallbackMethod;

impl CallbackMethod {
    /// Returns generated C parameters.
    pub fn c_parameters(&self) -> Vec<CallbackCParameter> {
        self.c_parameters.clone()
    }

    /// Returns the arguments passed to the static JVM callback method.
    pub fn jni_arguments(&self) -> ArgumentList {
        ArgumentList::from_iter(
            self.arguments
                .iter()
                .flat_map(CallbackArgument::jni_arguments)
                .chain(self.transfers_classes().then(|| {
                    Expression::identifier(
                        Identifier::parse("__boltffi_class_delivery")
                            .expect("generated delivery marker"),
                    )
                })),
        )
    }

    /// Returns byte-array callback arguments.
    pub fn byte_arrays(&self) -> Vec<CallbackBytesArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::bytes)
            .collect()
    }

    /// Returns direct-vector callback arguments.
    pub fn direct_vectors(&self) -> Vec<CallbackDirectVectorArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::direct_vector)
            .collect()
    }

    /// Returns direct-record callback arguments.
    pub fn record_arrays(&self) -> Vec<CallbackRecordArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::record)
            .collect()
    }

    /// Returns callback-handle callback arguments.
    pub fn callback_handles(&self) -> Vec<CallbackHandleArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::callback_handle)
            .collect()
    }

    /// Returns closure-handle callback arguments.
    pub fn closure_handles(&self) -> Vec<CallbackClosureArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::closure_handle)
            .collect()
    }

    /// Returns async callback completion arguments.
    pub fn completions(&self) -> Vec<CallbackCompletionArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::completion)
            .collect()
    }
}
