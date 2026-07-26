use boltffi_binding::{ClassDecl, Native};

use crate::{
    core::Result,
    target::java::{
        JavaHost,
        admission::{FunctionShape, ReceiverSupport},
        primitive::Primitive,
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ClassShape {
    Supported,
    HandleCarrier,
    Initializer(FunctionShape),
    Method(FunctionShape),
}

impl ClassShape {
    pub fn classify(declaration: &ClassDecl<Native>) -> Self {
        if Primitive::from_handle_carrier(declaration.handle()).is_err() {
            return Self::HandleCarrier;
        }
        declaration
            .initializers()
            .iter()
            .map(|initializer| {
                Self::Initializer(FunctionShape::classify_callable(
                    initializer.callable(),
                    ReceiverSupport::Forbidden,
                ))
            })
            .chain(declaration.methods().iter().map(|method| {
                Self::Method(FunctionShape::classify_callable(
                    method.callable(),
                    ReceiverSupport::Handle(declaration.handle()),
                ))
            }))
            .find(|shape| !shape.is_supported())
            .unwrap_or(Self::Supported)
    }

    pub fn require_supported(self) -> Result<()> {
        self.unsupported_reason()
            .map_or(Ok(()), |shape| Err(JavaHost::unsupported(shape)))
    }

    pub const fn unsupported_reason(self) -> Option<&'static str> {
        match self {
            Self::Supported => None,
            Self::HandleCarrier => Some("class handle carrier"),
            Self::Initializer(shape) | Self::Method(shape) => shape.unsupported_reason(),
        }
    }

    const fn is_supported(self) -> bool {
        match self {
            Self::Supported => true,
            Self::Initializer(shape) | Self::Method(shape) => shape.unsupported_reason().is_none(),
            Self::HandleCarrier => false,
        }
    }
}
