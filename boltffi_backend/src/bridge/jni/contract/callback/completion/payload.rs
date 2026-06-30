//! Success payloads carried by async callback completion.
//!
//! Async callback completion moves the callback result through a generated
//! native method instead of through the original vtable return. That result can
//! be void, a scalar, owned encoded bytes, a direct record, or a callback
//! handle. Each shape has a different JNI parameter and C argument expression.
//!
//! This module owns the payload contract for that later completion call. It
//! validates that the C return shape can be represented by a JVM completion
//! method and records the exact argument fragments needed to call Rust back.

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::JniType,
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// Successful async callback completion payload carried from the JVM to Rust.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionPayload {
    suffix: String,
    c_type: TypeFragment,
    jni_type: TypeFragment,
    jni_signature: String,
    kind: CallbackCompletionPayloadKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum CallbackCompletionPayloadKind {
    Scalar(JniType),
    Bytes,
    Record,
    CallbackHandle { create_handle: Identifier },
}

/// JVM value shape accepted by an async callback completion method.
///
/// The value is narrower than the C payload type. It names the kind of JVM
/// argument the generated completion method receives after the JNI bridge has
/// validated the C payload can cross the boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CallbackCompletionPayloadValue {
    /// A primitive JNI value.
    Scalar(JniType),
    /// An owned encoded byte buffer.
    Bytes,
    /// A direct record copied through a byte array.
    Record,
    /// A callback handle carried as a JVM long.
    CallbackHandle,
}

impl CallbackCompletionPayload {
    /// Creates a completion payload from the C completion function pointer payload type.
    pub fn from_c_type(ty: &c::Type, callbacks: &[c::Callback]) -> Result<Self> {
        match ty {
            c::Type::Buffer => Ok(Self {
                suffix: "Bytes".to_owned(),
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: TypeFragment::new("jbyteArray"),
                jni_signature: "[B".to_owned(),
                kind: CallbackCompletionPayloadKind::Bytes,
            }),
            c::Type::DirectRecord(name) => Ok(Self {
                suffix: format!("Record_{}", name.as_str()),
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: TypeFragment::new("jbyteArray"),
                jni_signature: "[B".to_owned(),
                kind: CallbackCompletionPayloadKind::Record,
            }),
            c::Type::CallbackHandle(callback) => {
                let declaration = callbacks
                    .iter()
                    .find(|declaration| declaration.id() == *callback)
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant:
                            "async callback completion payload has no C callback declaration",
                    })?;
                let create_handle = Identifier::parse(declaration.create_handle().name())?;
                let suffix = create_handle
                    .as_str()
                    .strip_prefix("boltffi_create_callback_")
                    .unwrap_or_else(|| create_handle.as_str());
                let c_type = TypeFragment::anonymous(ty)?;
                Ok(Self {
                    suffix: format!("Callback_{suffix}"),
                    c_type,
                    jni_type: TypeFragment::new("jlong"),
                    jni_signature: "J".to_owned(),
                    kind: CallbackCompletionPayloadKind::CallbackHandle { create_handle },
                })
            }
            c::Type::Void
            | c::Type::Status
            | c::Type::String
            | c::Type::Span
            | c::Type::FutureHandle
            | c::Type::Named(_)
            | c::Type::ConstPointer(_)
            | c::Type::MutPointer(_)
            | c::Type::FunctionPointer { .. } => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "async callback completion payload",
            }),
            ty @ (c::Type::Bool
            | c::Type::Int8
            | c::Type::Uint8
            | c::Type::Int16
            | c::Type::Uint16
            | c::Type::Int32
            | c::Type::Uint32
            | c::Type::Int64
            | c::Type::Uint64
            | c::Type::SignedPointerWidth
            | c::Type::PointerWidth
            | c::Type::Float32
            | c::Type::Float64
            | c::Type::StreamPollResult
            | c::Type::WaitResult
            | c::Type::CStyleEnum { .. }) => {
                let jni_type = JniType::from_c_type(ty)?;
                Ok(Self {
                    suffix: Self::scalar_suffix(ty)?,
                    c_type: TypeFragment::anonymous(ty)?,
                    jni_type: jni_type.as_type_fragment(),
                    jni_signature: jni_type.signature().to_owned(),
                    kind: CallbackCompletionPayloadKind::Scalar(jni_type),
                })
            }
        }
    }

    /// Returns the suffix used to deduplicate generated completion invokers.
    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// Returns the C payload type passed to the completion function pointer.
    pub fn c_type(&self) -> &TypeFragment {
        &self.c_type
    }

    /// Returns the JNI parameter type accepted by the success invoker.
    pub fn jni_type(&self) -> &TypeFragment {
        &self.jni_type
    }

    /// Returns the JVM method descriptor fragment for this payload.
    pub fn jni_signature(&self) -> &str {
        &self.jni_signature
    }

    /// Returns the JVM value shape carried by the completion method.
    pub fn value(&self) -> CallbackCompletionPayloadValue {
        match self.kind {
            CallbackCompletionPayloadKind::Scalar(jni_type) => {
                CallbackCompletionPayloadValue::Scalar(jni_type)
            }
            CallbackCompletionPayloadKind::Bytes => CallbackCompletionPayloadValue::Bytes,
            CallbackCompletionPayloadKind::Record => CallbackCompletionPayloadValue::Record,
            CallbackCompletionPayloadKind::CallbackHandle { .. } => {
                CallbackCompletionPayloadValue::CallbackHandle
            }
        }
    }

    /// Returns whether the payload is an owned byte buffer.
    pub fn is_bytes(&self) -> bool {
        matches!(self.kind, CallbackCompletionPayloadKind::Bytes)
    }

    /// Returns whether the payload is a direct record value.
    pub fn is_record(&self) -> bool {
        matches!(self.kind, CallbackCompletionPayloadKind::Record)
    }

    /// Returns the C callback constructor needed for callback-handle payloads.
    pub fn callback_handle_constructor(&self) -> Option<&Identifier> {
        match &self.kind {
            CallbackCompletionPayloadKind::CallbackHandle { create_handle } => Some(create_handle),
            _ => None,
        }
    }

    fn scalar_suffix(ty: &c::Type) -> Result<String> {
        Ok(match ty {
            c::Type::Bool => "Bool".to_owned(),
            c::Type::Int8 => "I8".to_owned(),
            c::Type::Uint8 | c::Type::StreamPollResult => "U8".to_owned(),
            c::Type::Int16 => "I16".to_owned(),
            c::Type::Uint16 => "U16".to_owned(),
            c::Type::Int32 => "I32".to_owned(),
            c::Type::Uint32 | c::Type::WaitResult => "U32".to_owned(),
            c::Type::Int64 => "I64".to_owned(),
            c::Type::Uint64 => "U64".to_owned(),
            c::Type::SignedPointerWidth => "ISize".to_owned(),
            c::Type::PointerWidth => "USize".to_owned(),
            c::Type::Float32 => "F32".to_owned(),
            c::Type::Float64 => "F64".to_owned(),
            c::Type::CStyleEnum { name, .. } => format!("Enum_{}", name.as_str()),
            c::Type::Void
            | c::Type::Status
            | c::Type::Buffer
            | c::Type::String
            | c::Type::Span
            | c::Type::FutureHandle
            | c::Type::CallbackHandle(_)
            | c::Type::Named(_)
            | c::Type::DirectRecord(_)
            | c::Type::ConstPointer(_)
            | c::Type::MutPointer(_)
            | c::Type::FunctionPointer { .. } => {
                return Err(Error::UnsupportedBridge {
                    bridge: JNI_BRIDGE,
                    shape: "scalar async callback completion payload",
                });
            }
        })
    }
}
