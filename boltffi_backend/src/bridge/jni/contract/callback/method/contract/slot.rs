//! JNI callback methods built from C callback vtable slots.
//!
//! The C bridge owns the callback ABI that Rust calls: the slot name, handle
//! parameter, grouped payload parameters, return type, and optional closure
//! return storage. The JVM bridge needs the same slot as a static Java method
//! call with a JVM descriptor, cached method id, typed JNI arguments, and return
//! handling.
//!
//! This module performs that translation for one slot. It validates the C slot
//! shape once and produces the callback method contract consumed by the callback
//! templates.

use crate::{
    bridge::{
        c::{self, Identifier},
        jni::{
            CallbackArgument, CallbackCParameter, CallbackClosureReturn, ClosureRegistration,
            JvmMethodReturn,
        },
    },
    core::{Error, Result},
};

use super::CallbackMethod;

const JNI_BRIDGE: &str = "jni";

impl CallbackMethod {
    /// Builds a JNI callback method from one C callback vtable slot.
    pub fn from_slot(
        stem: &str,
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        let Some(c::Type::Uint64) = slot.parameters().first().map(c::Parameter::ty) else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback vtable slot does not start with a uint64 handle",
            });
        };
        let (returns, closure_return) = Self::returns(slot, callbacks, closures)?;
        let arguments = Self::arguments(slot, callbacks, closures)?;
        let c_parameters: Vec<_> = slot
            .parameters()
            .iter()
            .map(CallbackCParameter::from_parameter)
            .collect::<Result<Vec<_>>>()?;
        let transfers_classes = c_parameters
            .iter()
            .any(|parameter| parameter.class_release().is_some());
        let signature = format!(
            "({}{}){}",
            arguments
                .iter()
                .map(CallbackArgument::jni_signature)
                .collect::<Vec<_>>()
                .join(""),
            if transfers_classes {
                "Ljava/nio/ByteBuffer;"
            } else {
                ""
            },
            returns.signature()
        );
        Ok(Self {
            function: Identifier::parse(format!("{stem}_{}", slot.name()))?,
            method: slot.name().clone(),
            method_id: Identifier::parse(format!("g_{stem}_{}_method", slot.name()))?,
            signature,
            returns,
            return_channel: slot.return_channel(),
            c_parameters,
            closure_return,
            arguments,
            transfers_classes,
        })
    }

    fn returns(
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<(JvmMethodReturn, Option<CallbackClosureReturn>)> {
        let closure_return = slot
            .parameter_groups()
            .iter()
            .filter_map(|group| match group {
                c::ParameterGroup::ClosureReturn(returned) => Some(returned),
                _ => None,
            })
            .map(|returned| CallbackClosureReturn::from_return(slot, returned, closures))
            .collect::<Result<Vec<_>>>()?;
        match closure_return.as_slice() {
            [] => JvmMethodReturn::from_c_type(slot.returns(), callbacks)
                .map(|returns| (returns, None)),
            [returned] if matches!(slot.returns(), c::Type::Status) => {
                Ok((JvmMethodReturn::closure_status()?, Some(returned.clone())))
            }
            [_] => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback closure return does not use FfiStatus",
            }),
            _ => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback method has multiple closure return out-pointers",
            }),
        }
    }

    fn arguments(
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Vec<CallbackArgument>> {
        slot.parameter_groups()
            .iter()
            .filter(|group| !matches!(group, c::ParameterGroup::ClosureReturn(_)))
            .map(|group| CallbackArgument::from_group(slot, group, callbacks, closures))
            .collect()
    }
}
