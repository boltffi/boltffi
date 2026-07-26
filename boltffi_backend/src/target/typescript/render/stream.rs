use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ByteSize, DirectValueType, DirectVectorElementType, EnumDecl, StreamDecl, StreamItemPlanRender,
    StreamMode, TypeRef, Wasm32, wasm32,
};

use crate::core::{Emitted, Error, RenderContext, Result};

use super::super::{
    codec::{ReadKind, Reader},
    name_style::Name,
    primitive::Scalar,
    syntax::{Expression, Identifier, MemberName, TypeName},
};
use super::{Type, direct_vector::DirectVector};

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/stream.ts", escape = "none")]
pub struct Stream {
    name: MemberName,
    factory: Identifier,
    owner: Option<TypeName>,
    item: Item,
    asynchronous: bool,
    batch: bool,
    callback: bool,
    subscribe: Identifier,
    pop_batch: Identifier,
    poll: Identifier,
    unsubscribe: Identifier,
    free: Identifier,
}

struct Item {
    ty: TypeName,
    size: u64,
    bulk: Option<Identifier>,
    direct_decode: Option<Expression>,
    encoded_decode: Option<Expression>,
    encoded_array: Option<Identifier>,
}

struct ItemRenderer<'context> {
    context: &'context RenderContext<'context, Wasm32>,
}

impl Stream {
    pub fn from_declaration(
        declaration: &StreamDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let mode = declaration.mode();
        let protocol = declaration.protocol();
        Ok(Self {
            name: Name::new(declaration.name()).member()?,
            factory: Identifier::parse(format!(
                "_{}Stream",
                Name::new(declaration.name()).identifier()?
            ))?,
            owner: declaration
                .owner()
                .map(|owner| {
                    context
                        .class(owner)
                        .map(|class| Name::new(class.name()).type_name())
                        .ok_or_else(|| Self::unsupported("stream owner without declaration"))
                })
                .transpose()?,
            item: declaration
                .item()
                .render_with(&mut ItemRenderer { context })?,
            asynchronous: matches!(mode, StreamMode::Async),
            batch: matches!(mode, StreamMode::Batch),
            callback: matches!(mode, StreamMode::Callback),
            subscribe: Identifier::parse(protocol.subscribe().name().as_str())?,
            pop_batch: Identifier::parse(protocol.pop_batch().name().as_str())?,
            poll: Identifier::parse(protocol.poll().name().as_str())?,
            unsubscribe: Identifier::parse(protocol.unsubscribe().name().as_str())?,
            free: Identifier::parse(protocol.free().name().as_str())?,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?))
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl<'plan> StreamItemPlanRender<'plan, Wasm32> for ItemRenderer<'_> {
    type Output = Result<Item>;

    fn direct(&mut self, ty: &'plan DirectValueType, size: ByteSize) -> Self::Output {
        let reader = Identifier::known("reader");
        let size = size.get();
        match ty {
            DirectValueType::Primitive(primitive) => {
                let element = DirectVectorElementType::primitive(*primitive)
                    .ok_or_else(|| Stream::unsupported("direct stream primitive"))?;
                let vector = DirectVector::outgoing(&element, self.context)?;
                Ok(Item {
                    ty: Type::primitive(*primitive)?,
                    size,
                    bulk: Some(vector.borrow_method()),
                    direct_decode: None,
                    encoded_decode: None,
                    encoded_array: None,
                })
            }
            DirectValueType::Record(id) => {
                let record = self
                    .context
                    .record(*id)
                    .ok_or_else(|| Stream::unsupported("stream record without declaration"))?;
                Ok(Item {
                    ty: Name::new(record.name()).type_name(),
                    size,
                    bulk: None,
                    direct_decode: Some(Expression::call(
                        Expression::identifier(Name::new(record.name()).codec_identifier()?),
                        Identifier::known("decode"),
                        [Expression::identifier(reader)].into_iter().collect(),
                    )),
                    encoded_decode: None,
                    encoded_array: None,
                })
            }
            DirectValueType::Enum(id) => {
                let enumeration = self
                    .context
                    .enumeration(*id)
                    .ok_or_else(|| Stream::unsupported("stream enum without declaration"))?;
                let EnumDecl::CStyle(enumeration) = enumeration else {
                    return Err(Stream::unsupported("direct data enum stream"));
                };
                Ok(Item {
                    ty: Name::new(enumeration.name()).type_name(),
                    size,
                    bulk: None,
                    direct_decode: Some(Expression::call(
                        Expression::identifier(reader),
                        Scalar::new(enumeration.repr().primitive())?.read_method(),
                        Default::default(),
                    )),
                    encoded_decode: None,
                    encoded_array: None,
                })
            }
            _ => Err(Stream::unsupported("direct stream item")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan boltffi_binding::ReadPlan,
        shape: wasm32::BufferShape,
    ) -> Self::Output {
        if !matches!(shape, wasm32::BufferShape::Packed) {
            return Err(Stream::unsupported("encoded stream item shape"));
        }
        let decoded =
            read.render_with(&mut Reader::new(Identifier::known("reader"), self.context))?;
        let encoded_array = match decoded.kind() {
            Some(ReadKind::Primitive(primitive)) => {
                Some(Scalar::new(primitive)?.read_array_method())
            }
            Some(
                ReadKind::String
                | ReadKind::Utf8String
                | ReadKind::Bytes
                | ReadKind::CustomPrimitive(_)
                | ReadKind::OptionalPrimitive(_)
                | ReadKind::ErrorRecord(_)
                | ReadKind::ErrorEnum(_),
            )
            | None => None,
        };
        Ok(Item {
            ty: Type::from_ref(ty, self.context)?,
            size: 0,
            bulk: None,
            direct_decode: None,
            encoded_decode: Some(decoded.into_expression()),
            encoded_array,
        })
    }
}
