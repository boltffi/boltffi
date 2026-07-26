use boltffi_binding::{CodecNode, Primitive};

use crate::experimental::error::Error;

pub mod incoming;
pub mod outgoing;

mod custom;

pub use self::custom::{BorrowedOutgoing, Incoming, Outgoing};

pub fn require_runtime_wire(codec: &CodecNode) -> Result<(), Error> {
    RuntimeWireCodec.require_supported(codec)
}

struct RuntimeWireCodec;

impl RuntimeWireCodec {
    const MAX_TUPLE_ARITY: usize = 12;

    fn require_supported(&self, codec: &CodecNode) -> Result<(), Error> {
        match codec {
            CodecNode::Primitive(_)
            | CodecNode::String
            | CodecNode::Utf8String
            | CodecNode::InternedString { .. }
            | CodecNode::Bytes
            | CodecNode::DirectRecord(_)
            | CodecNode::EncodedRecord(_)
            | CodecNode::CStyleEnum(_)
            | CodecNode::DataEnum(_)
            | CodecNode::Custom { .. }
            | CodecNode::Builtin(_) => Ok(()),
            CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
                self.require_supported(inner)
            }
            CodecNode::Result { ok, err } => {
                self.require_supported(ok)?;
                self.require_supported(err)
            }
            CodecNode::Tuple(elements) if elements.len() <= Self::MAX_TUPLE_ARITY => elements
                .iter()
                .try_for_each(|element| self.require_supported(element)),
            CodecNode::Tuple(_) => Err(Error::UnsupportedExpansion("encoded tuple arity")),
            CodecNode::Map { key, value, .. } => {
                self.require_supported_map_key(key)?;
                self.require_supported(value)
            }
            CodecNode::ClassHandle(_) | CodecNode::CallbackHandle(_) | _ => {
                Err(Error::UnsupportedExpansion("codec node"))
            }
        }
    }

    fn require_supported_map_key(&self, codec: &CodecNode) -> Result<(), Error> {
        match codec {
            CodecNode::Primitive(Primitive::F32 | Primitive::F64) => Err(
                Error::UnsupportedExpansion("floating-point encoded map key"),
            ),
            CodecNode::Custom { .. } => Err(Error::UnsupportedExpansion("custom encoded map key")),
            CodecNode::Map { .. } => Err(Error::UnsupportedExpansion("nested encoded map key")),
            CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
                self.require_supported_map_key(inner)
            }
            CodecNode::Result { ok, err } => {
                self.require_supported_map_key(ok)?;
                self.require_supported_map_key(err)
            }
            CodecNode::Tuple(elements) if elements.len() <= Self::MAX_TUPLE_ARITY => elements
                .iter()
                .try_for_each(|element| self.require_supported_map_key(element)),
            CodecNode::Tuple(_) => Err(Error::UnsupportedExpansion("encoded tuple arity")),
            _ => self.require_supported(codec),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::require_runtime_wire;
    use boltffi_binding::CodecNode;

    #[test]
    fn runtime_wire_accepts_nested_interned_strings() {
        let codec = CodecNode::Optional(Box::new(CodecNode::InternedString {
            static_values: vec!["Chrome".to_owned()],
        }));

        assert!(require_runtime_wire(&codec).is_ok());
    }
}
