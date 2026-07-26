use boltffi_binding::{Native, RecordDecl};

use crate::core::{Error, Result};

use super::{FunctionShape, ReceiverSupport};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecordShape {
    Supported,
    Associated(FunctionShape),
    Unknown,
}

impl RecordShape {
    pub fn classify(declaration: &RecordDecl<Native>) -> Self {
        match declaration {
            RecordDecl::Direct(_) => Self::classify_calls(declaration, ReceiverSupport::Direct),
            RecordDecl::Encoded(_) => Self::classify_calls(declaration, ReceiverSupport::Encoded),
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
            Self::Unknown => Some("unknown record declaration"),
        }
    }

    fn classify_calls(declaration: &RecordDecl<Native>, receiver: ReceiverSupport) -> Self {
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
