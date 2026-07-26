use boltffi_binding::{EnumDecl, Native, Receive};

use crate::core::{Error, Result};

use super::{FunctionShape, ReceiverSupport};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EnumShape {
    Supported,
    Associated(FunctionShape),
    MutableCStyleReceiver,
    Unknown,
}

impl EnumShape {
    pub fn classify(declaration: &EnumDecl<Native>) -> Self {
        match declaration {
            EnumDecl::CStyle(enumeration)
                if enumeration
                    .methods()
                    .iter()
                    .any(|method| method.callable().receiver() == Some(Receive::ByMutRef)) =>
            {
                Self::MutableCStyleReceiver
            }
            EnumDecl::CStyle(_) => Self::classify_calls(declaration, ReceiverSupport::Direct),
            EnumDecl::Data(_) => Self::classify_calls(declaration, ReceiverSupport::Encoded),
            _ => Self::Unknown,
        }
    }

    pub fn require_supported(self) -> Result<()> {
        self.unsupported_reason().map_or(Ok(()), |shape| {
            Err(Error::UnsupportedTarget {
                target: "java",
                shape,
            })
        })
    }

    pub const fn unsupported_reason(self) -> Option<&'static str> {
        match self {
            Self::Supported => None,
            Self::Associated(shape) => shape.unsupported_reason(),
            Self::MutableCStyleReceiver => Some("mutable c-style enum receiver"),
            Self::Unknown => Some("unknown enum declaration"),
        }
    }

    fn classify_calls(declaration: &EnumDecl<Native>, receiver: ReceiverSupport) -> Self {
        declaration
            .initializers()
            .iter()
            .map(|initializer| initializer.callable())
            .chain(declaration.methods().iter().map(|method| method.callable()))
            .map(|callable| FunctionShape::classify_callable(callable, receiver))
            .find(|shape| shape.unsupported_reason().is_some())
            .map_or(Self::Supported, Self::Associated)
    }
}
