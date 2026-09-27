//! Contract for one registered closure signature.
//!
//! A closure signature can appear in many declarations, but the JNI bridge
//! should generate one bridge class and one native call/release pair for it.
//! That registration must know the C function pointer type, JVM class name,
//! arguments, return path, and optional callback-handle helpers.
//!
//! This module stores that complete registration. Templates render from this
//! contract instead of asking whether a closure came from a function parameter,
//! callback method, nested closure, or returned closure. The origin is not the
//! identity. The signature is the identity, and the registration is the one
//! place that binds that signature to generated JNI symbols.

mod build;
mod index;

use boltffi_binding::ClosureSignature;

use crate::bridge::{
    c::{self, Identifier, TypeFragment},
    jni::{
        CallbackClosureHandle, ClosureArgument, JvmClassPath, JvmMethodReturn, SuccessOutArgument,
    },
};

/// A JNI trampoline registration for one inline closure signature.
///
/// The registration owns the generated JVM class, cached method ids, C call and
/// release symbols, argument conversions, return conversion, and optional handle
/// helpers for callbacks that carry Rust-owned closures back to Java.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureRegistration {
    signature: ClosureSignature,
    class: JvmClassPath,
    global_class: Identifier,
    call_method: Identifier,
    free_method: Identifier,
    load: Identifier,
    unload: Identifier,
    call: Identifier,
    release: Identifier,
    callback_handle: Option<CallbackClosureHandle>,
    returns: JvmMethodReturn,
    return_channel: c::ReturnChannel,
    arguments: Vec<ClosureArgument>,
}

impl ClosureRegistration {
    pub(crate) fn returns_error(&self) -> bool {
        self.return_channel == c::ReturnChannel::EncodedError
    }

    /// Returns the closure invocation signature.
    pub fn signature(&self) -> &ClosureSignature {
        &self.signature
    }

    /// Returns the JVM closure bridge class.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the global class reference symbol.
    pub fn global_class(&self) -> &Identifier {
        &self.global_class
    }

    /// Returns the cached static `call` method id symbol.
    pub fn call_method(&self) -> &Identifier {
        &self.call_method
    }

    /// Returns the cached static `free` method id symbol.
    pub fn free_method(&self) -> &Identifier {
        &self.free_method
    }

    /// Returns the load hook called from `JNI_OnLoad`.
    pub fn load(&self) -> &Identifier {
        &self.load
    }

    /// Returns the unload hook called from `JNI_OnUnload`.
    pub fn unload(&self) -> &Identifier {
        &self.unload
    }

    /// Returns the generated C closure invoke function.
    pub fn call(&self) -> &Identifier {
        &self.call
    }

    /// Returns the generated C closure release function.
    pub fn release(&self) -> &Identifier {
        &self.release
    }

    /// Returns the Rust-owned closure handle contract for callback arguments.
    pub fn callback_handle(&self) -> Option<&CallbackClosureHandle> {
        self.callback_handle.as_ref()
    }

    /// Returns the C return type for the closure invoke function.
    pub fn c_return_type(&self) -> &TypeFragment {
        self.returns.c_type()
    }

    /// Returns the JNI return type for the native closure-call export.
    pub fn callback_return_type(&self) -> TypeFragment {
        self.returns.jni_type()
    }

    /// Returns whether the closure invoke function returns no value.
    pub fn returns_void(&self) -> bool {
        self.returns.is_void()
    }

    /// Returns whether the JVM closure method returns a byte array.
    pub fn returns_byte_array(&self) -> bool {
        self.returns.returns_byte_array()
    }

    /// Returns whether the JVM closure method returns owned encoded bytes.
    pub fn returns_bytes(&self) -> bool {
        self.returns.returns_bytes()
    }

    /// Returns whether the JVM closure method returns a direct record byte array.
    pub fn returns_record(&self) -> bool {
        self.returns.returns_record()
    }

    /// Returns whether the JVM closure method returns a callback handle token.
    pub fn returns_callback_handle(&self) -> bool {
        self.returns.returns_callback_handle()
    }

    /// Returns the C callback handle constructor for callback handle returns.
    pub fn callback_handle_constructor(&self) -> Option<&Identifier> {
        self.returns.callback_handle_constructor()
    }

    /// Returns the JNI method descriptor.
    pub fn method_signature(&self) -> String {
        format!(
            "(J{}){}",
            self.arguments
                .iter()
                .map(ClosureArgument::jni_signature)
                .collect::<Vec<_>>()
                .join(""),
            self.returns.signature()
        )
    }

    /// Returns the `CallStatic*Method` suffix for non-void closure returns.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        self.returns.call_method_suffix()
    }

    /// Returns the C value returned when JVM dispatch fails.
    pub fn failure_value(&self) -> Option<c::Expression> {
        self.returns.failure_value()
    }

    /// Returns the JNI value returned when a Rust-owned closure call fails.
    pub fn callback_failure_value(&self) -> Option<c::Expression> {
        self.returns.jni_failure_value()
    }

    /// Returns generated C closure arguments.
    pub fn arguments(&self) -> &[ClosureArgument] {
        &self.arguments
    }

    /// Returns the success out argument for fallible closure success values.
    pub fn success_out(&self) -> Option<SuccessOutArgument> {
        self.arguments.iter().find_map(ClosureArgument::success_out)
    }
}
