//! Registered closure contracts built from C bridge closure signatures.
//!
//! Inline closures appear in several places: exported function parameters,
//! returned closures, callback method parameters, callback method returns, and
//! nested closure arguments. The C bridge describes each occurrence as a
//! signature with call, context, and release behavior.
//!
//! This module turns those repeated C shapes into deduplicated JNI closure
//! registrations. Each registration owns the generated JVM class path, call and
//! release symbols, argument contract, return contract, and optional callback
//! handle wrapper for that signature.

use std::collections::BTreeSet;

use boltffi_binding::{CallbackId, ClosureSignature};

use crate::{
    bridge::{
        c,
        jni::{CallbackClosureHandle, ClosureArgument, JvmClassPath, JvmMethodReturn},
    },
    core::{Error, Result},
};

use super::super::super::names::ClosureNames;
use super::ClosureRegistration;
use super::index::ClosureRegistrationIndex;

impl ClosureRegistration {
    /// Builds unique closure registrations from C functions and callback slots.
    pub fn from_c_bridge(
        class: &JvmClassPath,
        functions: &[c::Function],
        callbacks: &[c::Callback],
        returned_callbacks: &BTreeSet<CallbackId>,
    ) -> Result<Vec<Self>> {
        ClosureRegistrationIndex::from_c_bridge(class, functions, callbacks, returned_callbacks)
            .map(ClosureRegistrationIndex::into_registrations)
    }
}

pub struct ClosureRegistrationConstructor;

impl ClosureRegistrationConstructor {
    pub fn from_closure_parameter(
        class: &JvmClassPath,
        call_type: &c::Type,
        closure: &c::ClosureParameter,
        callback_argument: bool,
        callbacks: &[c::Callback],
    ) -> Result<ClosureRegistration> {
        Self::from_c_group(
            class,
            call_type,
            closure.signature(),
            closure.return_channel(),
            callback_argument,
            callbacks,
            closure
                .parameter_groups()
                .iter()
                .map(|group| ClosureArgument::from_closure_group(class, closure, group))
                .collect::<Result<Vec<_>>>()?,
        )
    }

    pub fn from_closure_return(
        class: &JvmClassPath,
        returned: &c::ClosureReturnParameter,
        callback_handle: bool,
        callbacks: &[c::Callback],
    ) -> Result<ClosureRegistration> {
        Self::from_c_group(
            class,
            returned.call_type(),
            returned.signature(),
            returned.return_channel(),
            callback_handle,
            callbacks,
            returned
                .parameter_groups()
                .iter()
                .map(|group| ClosureArgument::from_return_group(class, returned, group))
                .collect::<Result<Vec<_>>>()?,
        )
    }

    pub fn retain_callback_handle(
        registration: &mut ClosureRegistration,
        class: &JvmClassPath,
        call_type: &c::Type,
    ) -> Result<()> {
        if registration.callback_handle.is_none() {
            registration.callback_handle = Some(CallbackClosureHandle::new(
                class,
                &registration.signature,
                call_type,
            )?);
        }
        Ok(())
    }

    fn from_c_group(
        class: &JvmClassPath,
        call_type: &c::Type,
        signature: &ClosureSignature,
        return_channel: c::ReturnChannel,
        callback_argument: bool,
        callbacks: &[c::Callback],
        arguments: Vec<ClosureArgument>,
    ) -> Result<ClosureRegistration> {
        let c::Type::FunctionPointer { returns, params } = call_type else {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "closure call parameter is not a function pointer",
            });
        };
        if !matches!(
            params.first(),
            Some(c::Type::MutPointer(inner)) if matches!(inner.as_ref(), c::Type::Void)
        ) {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "closure call parameter does not start with void context",
            });
        }
        let names = ClosureNames::new(signature);
        Ok(ClosureRegistration {
            signature: signature.clone(),
            class: class.closure_class(signature)?,
            global_class: names.global_class()?,
            call_method: names.call_method()?,
            free_method: names.free_method()?,
            load: names.load()?,
            unload: names.unload()?,
            call: names.call()?,
            release: names.release()?,
            callback_handle: callback_argument
                .then(|| CallbackClosureHandle::new(class, signature, call_type))
                .transpose()?,
            returns: JvmMethodReturn::from_c_type(returns, callbacks)?,
            arguments,
            return_channel,
        })
    }
}
