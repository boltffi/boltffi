//! Crate-wide build pass for the JNI bridge contract.
//!
//! JNI glue is one generated C file, but its pieces are connected. A native
//! method can use a closure signature that is also used by a callback method. A
//! callback can need an async completion helper. Stream protocol functions must
//! not also appear as normal native methods.
//!
//! This module performs that whole-file pass once. It reads the lower C bridge
//! contract, registers shared closure signatures, builds callback registrations,
//! filters stream protocol functions out of the native method list, and records
//! the support symbols the final source file needs.

use std::collections::BTreeSet;

use boltffi_binding::{CallbackId, DeclarationId};

use crate::{
    bridge::{
        c::{self, HeaderInclude, Identifier},
        jni::JvmClassPath,
    },
    core::{BridgeCapability, BridgeContract, FilePath, Result},
};

use super::{
    CallbackCompletionInvoker, CallbackHandleLifecycle, CallbackRegistration, ClosureRegistration,
    JniBridgeContract, NativeMethod, StreamProtocolMethods, SuccessOutWriter, index::SourceIndex,
};

impl JniBridgeContract {
    /// Builds the JNI bridge contract from the C bridge contract.
    pub fn from_c_bridge(
        class: JvmClassPath,
        source_path: FilePath,
        c_bridge: &c::CBridgeContract,
    ) -> Result<Self> {
        let handle_method_callbacks = Self::handle_method_callbacks(c_bridge);
        let closures = ClosureRegistration::from_c_bridge(
            &class,
            c_bridge.functions(),
            c_bridge.callbacks(),
            &handle_method_callbacks,
        )?;
        let callbacks = c_bridge
            .callbacks()
            .iter()
            .map(|callback| {
                CallbackRegistration::from_c_callback(
                    &class,
                    callback,
                    c_bridge.callbacks(),
                    &closures,
                    handle_method_callbacks.contains(&callback.id()),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let callback_handle_lifecycle = (!handle_method_callbacks.is_empty())
            .then(|| CallbackHandleLifecycle::new(&class))
            .transpose()?;
        let callback_completions = CallbackCompletionInvoker::from_callbacks(&class, &callbacks)?;
        let success_out_writers = SuccessOutWriter::from_c_bridge(&class, c_bridge)?;
        let methods = c_bridge
            .functions()
            .iter()
            .filter(|function| {
                !matches!(
                    function.source_declaration(),
                    Some(DeclarationId::Stream(_))
                )
            })
            .map(|function| NativeMethod::new(&class, function, c_bridge.callbacks(), &closures))
            .collect::<Result<Vec<_>>>()?;
        let streams = c_bridge
            .streams()
            .iter()
            .map(|stream| {
                StreamProtocolMethods::from_c_stream(
                    &class,
                    stream,
                    c_bridge.callbacks(),
                    &closures,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let source_index = SourceIndex::new(&callbacks, &methods, &streams)?;
        Ok(Self {
            capabilities: c_bridge
                .capabilities()
                .clone()
                .stable(BridgeCapability::Jni),
            c_header: HeaderInclude::from_files(&source_path, c_bridge.header_path())?,
            free_buffer: Identifier::parse(c_bridge.support().buffer_free()?.name())?,
            buffer_with_len: Identifier::parse(c_bridge.support().buffer_with_len()?.name())?,
            callback_handle_lifecycle,
            callbacks,
            callback_completions,
            success_out_writers,
            methods,
            streams,
            source_index,
            closures,
            class,
            source_path,
        })
    }

    fn handle_method_callbacks(c_bridge: &c::CBridgeContract) -> BTreeSet<CallbackId> {
        c_bridge
            .functions()
            .iter()
            .filter_map(Self::returned_callback_handle)
            .chain(
                c_bridge
                    .callbacks()
                    .iter()
                    .flat_map(Self::callback_argument_handles),
            )
            .collect()
    }

    fn returned_callback_handle(function: &c::Function) -> Option<CallbackId> {
        match function.returns() {
            c::Type::CallbackHandle(callback) => Some(*callback),
            _ => None,
        }
    }

    fn callback_argument_handles(callback: &c::Callback) -> impl Iterator<Item = CallbackId> + '_ {
        callback
            .methods()
            .iter()
            .flat_map(|method| method.parameters().iter())
            .filter_map(|parameter| match parameter.ty() {
                c::Type::CallbackHandle(callback) => Some(*callback),
                _ => None,
            })
    }
}
