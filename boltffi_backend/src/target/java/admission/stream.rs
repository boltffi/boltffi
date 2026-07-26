use boltffi_binding::{
    ByteSize, DirectValueType, Native, ReadPlan, StreamDecl, StreamItemPlanRender, TypeRef, native,
};

use crate::{
    core::{Error, Result},
    target::java::{JavaHost, primitive::Primitive},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StreamShape {
    Supported,
    Owner,
    Handle,
    Primitive,
    DirectItem,
    EncodedShape,
}

struct ItemShape;

impl StreamShape {
    pub fn classify(declaration: &StreamDecl<Native>) -> Self {
        if declaration.owner().is_none() {
            return Self::Owner;
        }
        if Primitive::from_handle_carrier(declaration.handle()).is_err() {
            return Self::Handle;
        }
        declaration.item().render_with(&mut ItemShape)
    }

    pub fn require_supported(self) -> Result<()> {
        self.unsupported_reason().map_or(Ok(()), |shape| {
            Err(Error::UnsupportedTarget {
                target: JavaHost::TARGET,
                shape,
            })
        })
    }

    pub const fn unsupported_reason(self) -> Option<&'static str> {
        match self {
            Self::Supported => None,
            Self::Owner => Some("stream without a Java class owner"),
            Self::Handle => Some("stream handle carrier"),
            Self::Primitive => Some("primitive Java stream item"),
            Self::DirectItem => Some("direct Java stream item"),
            Self::EncodedShape => Some("encoded Java stream item shape"),
        }
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for ItemShape {
    type Output = StreamShape;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) if Primitive::try_from(*primitive).is_ok() => {
                StreamShape::Supported
            }
            DirectValueType::Primitive(_) => StreamShape::Primitive,
            DirectValueType::Record(_) | DirectValueType::Enum(_) => StreamShape::Supported,
            _ => StreamShape::DirectItem,
        }
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => StreamShape::Supported,
            _ => StreamShape::EncodedShape,
        }
    }
}
