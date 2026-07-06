use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ByteSize, CanonicalName, DirectValueType, DirectVectorElementType, EnumId, Native, Primitive,
    ReadPlan, RecordId, StreamDecl, StreamItemPlan, StreamItemPlanRender, StreamMode, TypeRef,
    native,
};

use crate::{
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::Reader,
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            class::Class, direct_vector::DirectVector, enumeration::Enumeration, record::Record,
            type_name::KotlinType,
        },
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/stream.kt", escape = "none")]
struct StreamTemplate {
    stream: Stream,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    name: Identifier,
    receiver: Option<TypeName>,
    item: TypeName,
    delivery: Delivery,
    subscribe: Identifier,
    pop_batch: Identifier,
    wait: Identifier,
    poll: Identifier,
    unsubscribe: Identifier,
    free: Identifier,
    item_setup: Vec<Statement>,
    items: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Delivery {
    Async,
    Batch { subscription: TypeName },
    Callback { cancellable: TypeName },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StreamItem {
    ty: TypeName,
    setup: Vec<Statement>,
    items: Expression,
}

struct StreamItemRenderer<'context> {
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
}

impl Stream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let item = StreamItem::from_plan(declaration.item(), host, context)?;
        Ok(Self {
            name: Name::new(declaration.name()).function()?,
            receiver: declaration
                .owner()
                .map(|owner| Class::type_name_from_id(owner, context))
                .transpose()?,
            item: item.ty,
            delivery: Delivery::new(declaration.mode(), declaration.name())?,
            subscribe: Self::native_method(declaration.protocol().subscribe().name().as_str())?,
            pop_batch: Self::native_method(declaration.protocol().pop_batch().name().as_str())?,
            wait: Self::native_method(declaration.protocol().wait().name().as_str())?,
            poll: Self::native_method(declaration.protocol().poll().name().as_str())?,
            unsubscribe: Self::native_method(declaration.protocol().unsubscribe().name().as_str())?,
            free: Self::native_method(declaration.protocol().free().name().as_str())?,
            item_setup: item.setup,
            items: item.items,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            StreamTemplate { stream: self }.render()?.trim().to_owned(),
        ))
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn receiver(&self) -> Option<&TypeName> {
        self.receiver.as_ref()
    }

    pub fn item(&self) -> &TypeName {
        &self.item
    }

    pub fn async_delivery(&self) -> bool {
        matches!(self.delivery, Delivery::Async)
    }

    pub fn batch_subscription(&self) -> Option<&TypeName> {
        match &self.delivery {
            Delivery::Batch { subscription } => Some(subscription),
            Delivery::Async | Delivery::Callback { .. } => None,
        }
    }

    pub fn callback_cancellable(&self) -> Option<&TypeName> {
        match &self.delivery {
            Delivery::Callback { cancellable } => Some(cancellable),
            Delivery::Async | Delivery::Batch { .. } => None,
        }
    }

    pub fn subscribe(&self) -> &Identifier {
        &self.subscribe
    }

    pub fn pop_batch(&self) -> &Identifier {
        &self.pop_batch
    }

    pub fn wait(&self) -> &Identifier {
        &self.wait
    }

    pub fn poll(&self) -> &Identifier {
        &self.poll
    }

    pub fn unsubscribe(&self) -> &Identifier {
        &self.unsubscribe
    }

    pub fn free(&self) -> &Identifier {
        &self.free
    }

    pub fn item_setup(&self) -> &[Statement] {
        &self.item_setup
    }

    pub fn items(&self) -> &Expression {
        &self.items
    }

    fn native_method(name: &str) -> Result<Identifier> {
        Identifier::escape(name)
    }
}

impl Delivery {
    fn new(mode: StreamMode, name: &CanonicalName) -> Result<Self> {
        let name = Name::new(name).type_name();
        Ok(match mode {
            StreamMode::Async => Self::Async,
            StreamMode::Batch => Self::Batch {
                subscription: TypeName::new(format!("{name}Subscription")),
            },
            StreamMode::Callback => Self::Callback {
                cancellable: TypeName::new(format!("{name}Cancellable")),
            },
            _ => {
                return Err(KotlinHost::unsupported("unknown stream mode"));
            }
        })
    }
}

impl StreamItem {
    fn from_plan(
        plan: &StreamItemPlan<Native>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut StreamItemRenderer { host, context })
    }
}

impl StreamItemRenderer<'_> {
    fn direct_primitive_items(&self, primitive: Primitive) -> Result<Expression> {
        let bytes = Expression::identifier(Identifier::parse("bytes")?);
        let method = match primitive {
            Primitive::Bool => "readBooleanArray",
            Primitive::I8 => "readByteArray",
            Primitive::I16 => "readShortArray",
            Primitive::U16 => "readUShortArray",
            Primitive::I32 => "readIntArray",
            Primitive::U32 => "readUIntArray",
            Primitive::I64 | Primitive::ISize => "readLongArray",
            Primitive::U64 | Primitive::USize => "readULongArray",
            Primitive::F32 => "readFloatArray",
            Primitive::F64 => "readDoubleArray",
            Primitive::U8 => {
                return Ok(self
                    .direct_vector_call("readByteArray", bytes)?
                    .convert(Identifier::parse("toUByteArray")?)
                    .convert(Identifier::parse("toList")?));
            }
            _ => {
                return Err(KotlinHost::unsupported("unknown direct stream primitive"));
            }
        };
        let items = self.direct_vector_call(method, bytes)?;
        Ok(items.convert(Identifier::parse("toList")?))
    }

    fn direct_vector_call(&self, method: &str, bytes: Expression) -> Result<Expression> {
        Ok(Expression::call(
            "DirectVectorCodec",
            Identifier::parse(method)?,
            [bytes].into_iter().collect::<ArgumentList>(),
        ))
    }

    fn direct_record_items(&self, record: RecordId) -> Result<Expression> {
        DirectVector::from_element(&DirectVectorElementType::record(record), self.context)?
            .decode_byte_array(Expression::identifier(Identifier::parse("bytes")?))
    }

    fn direct_enum_items(&self, enumeration: EnumId) -> Result<Expression> {
        let enumeration = Enumeration::from_id(enumeration, self.host, self.context)?;
        let value = Identifier::parse("value")?;
        let items = self.direct_primitive_items(enumeration.repr()?)?;
        Ok(items.map(
            value.clone(),
            Expression::call(
                enumeration.name().clone(),
                Identifier::parse("fromValue")?,
                [Expression::identifier(value)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        ))
    }

    fn encoded_setup(&self) -> Result<Vec<Statement>> {
        let reader = Identifier::parse("reader")?;
        let count = Identifier::parse("count")?;
        Ok(vec![
            Statement::value(
                reader.clone(),
                Expression::construct(
                    TypeName::new("WireReader"),
                    [Expression::identifier(Identifier::parse("bytes")?)]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            ),
            Statement::value(
                count.clone(),
                Expression::call(
                    Expression::call(
                        Expression::identifier(reader),
                        Identifier::parse("readU32")?,
                        ArgumentList::default(),
                    ),
                    Identifier::parse("toInt")?,
                    ArgumentList::default(),
                ),
            ),
        ])
    }

    fn encoded_items(&self, read: &ReadPlan) -> Result<Expression> {
        let reader = Identifier::parse("reader")?;
        let count = Identifier::parse("count")?;
        let item = read
            .render_with(&mut Reader::new(reader, self.host, self.context))?
            .into_expression();
        Ok(Expression::list(Expression::identifier(count), item))
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for StreamItemRenderer<'_> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => Ok(StreamItem {
                ty: KotlinPrimitive::new(*primitive).api_type()?,
                setup: Vec::new(),
                items: self.direct_primitive_items(*primitive)?,
            }),
            DirectValueType::Record(record) => Ok(StreamItem {
                ty: Record::type_name_from_id(*record, self.context)?,
                setup: Vec::new(),
                items: self.direct_record_items(*record)?,
            }),
            DirectValueType::Enum(enumeration) => Ok(StreamItem {
                ty: Enumeration::type_name_from_id(*enumeration, self.context)?,
                setup: Vec::new(),
                items: self.direct_enum_items(*enumeration)?,
            }),
            _ => Err(KotlinHost::unsupported("unknown direct stream item")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => Ok(StreamItem {
                ty: KotlinType::type_ref(ty, self.context)?,
                setup: self.encoded_setup()?,
                items: self.encoded_items(read)?,
            }),
            _ => Err(KotlinHost::unsupported("encoded stream item shape")),
        }
    }
}
