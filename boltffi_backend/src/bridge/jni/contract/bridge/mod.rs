//! Crate-level JNI contract for one generated source file.
//!
//! `JniBridgeContract` is the finished JNI view of a crate. It names the owner
//! JVM class, generated source path, included C header, buffer release function,
//! and every native method, callback registration, closure registration, stream
//! protocol, and async callback completion invoker that must appear in the C
//! file.
//!
//! This is the only JNI contract module that sees the whole C bridge contract.
//! It performs the crate-wide pass once, then hands each feature module the
//! smaller contract pieces it owns. That prevents templates and leaf modules
//! from asking broad questions such as which declarations exist, whether stream
//! helpers are needed, or which closure signatures must be registered.

mod build;
mod index;

use boltffi_binding::{CallbackId, Native, SymbolId};

use crate::{
    bridge::{
        c::{HeaderInclude, Identifier},
        jni::JvmClassPath,
    },
    core::{BridgeCapabilities, BridgeContract, FilePath, contract::sealed},
};

use super::{
    CallbackCompletionInvoker, CallbackHandleLifecycle, CallbackRegistration, ClosureRegistration,
    DirectStreamBatchMethod, NativeMethod, StreamProtocolMethods, SuccessOutWriter,
};
use index::SourceIndex;

/// A complete JNI bridge contract for one generated C source file.
///
/// The value is built from one `CBridgeContract` and is ready for rendering. It
/// is intentionally crate-wide because JNI source generation needs shared
/// tables for callback classes, closure signatures, lifecycle hooks, and stream
/// helpers before individual method bodies can be printed.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct JniBridgeContract {
    capabilities: BridgeCapabilities,
    class: JvmClassPath,
    source_path: FilePath,
    c_header: HeaderInclude,
    free_buffer: Identifier,
    buffer_with_len: Identifier,
    callback_handle_lifecycle: Option<CallbackHandleLifecycle>,
    callbacks: Vec<CallbackRegistration>,
    callback_completions: Vec<CallbackCompletionInvoker>,
    success_out_writers: Vec<SuccessOutWriter>,
    closures: Vec<ClosureRegistration>,
    methods: Vec<NativeMethod>,
    streams: Vec<StreamProtocolMethods>,
    source_index: SourceIndex,
}

impl JniBridgeContract {
    /// Returns the JVM class that owns generated native methods.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the generated JNI source path.
    pub fn source_path(&self) -> &FilePath {
        &self.source_path
    }

    /// Returns the C header include path used by the JNI source.
    pub fn c_header(&self) -> &HeaderInclude {
        &self.c_header
    }

    /// Returns the C support function that releases owned BoltFFI byte buffers.
    pub fn free_buffer(&self) -> &Identifier {
        &self.free_buffer
    }

    pub(crate) fn buffer_with_len(&self) -> &Identifier {
        &self.buffer_with_len
    }

    /// Returns callback handle lifecycle methods when Rust-owned callback handles are exposed.
    pub fn callback_handle_lifecycle(&self) -> Option<&CallbackHandleLifecycle> {
        self.callback_handle_lifecycle.as_ref()
    }

    /// Returns generated callback registrations.
    pub fn callbacks(&self) -> &[CallbackRegistration] {
        &self.callbacks
    }

    /// Returns the JNI registration selected for a source callback.
    pub fn source_callback(&self, callback: CallbackId) -> Option<&CallbackRegistration> {
        self.source_index.callback(callback, &self.callbacks)
    }

    /// Returns async callback completion invokers.
    pub fn callback_completions(&self) -> &[CallbackCompletionInvoker] {
        &self.callback_completions
    }

    /// Returns helper methods that write fallible success values.
    pub fn success_out_writers(&self) -> &[SuccessOutWriter] {
        &self.success_out_writers
    }

    /// Returns generated closure trampoline registrations.
    pub fn closures(&self) -> &[ClosureRegistration] {
        &self.closures
    }

    /// Returns generated native methods.
    pub fn methods(&self) -> &[NativeMethod] {
        &self.methods
    }

    /// Returns the JNI native method selected for a source symbol.
    pub fn source_method(&self, symbol: SymbolId) -> Option<&NativeMethod> {
        self.source_index
            .method(symbol, &self.methods, &self.streams)
    }

    /// Returns the direct stream batch method selected for a source symbol.
    pub fn source_direct_batch(&self, symbol: SymbolId) -> Option<&DirectStreamBatchMethod> {
        self.source_index.direct_batch(symbol, &self.streams)
    }

    /// Returns generated stream protocol methods.
    pub fn streams(&self) -> &[StreamProtocolMethods] {
        &self.streams
    }
}

impl BridgeContract for JniBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for JniBridgeContract {}
