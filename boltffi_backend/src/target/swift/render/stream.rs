use askama::Template;
use boltffi_binding::{
    ByteSize, CanonicalName, DirectValueType, Native, Primitive, ReadPlan, RecordId, StreamDecl,
    StreamItemPlan, StreamItemPlanRender, StreamMode, TypeRef, native,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{AuxChunk, Emitted, Error, HelperId, RenderContext, Result, TextChunk},
    target::swift::{
        SwiftHost,
        codec::{ReadExpression, Reader},
        name_style::{GeneratedLocal, Name},
        primitive::SwiftPrimitive,
        render::{Documentation, SwiftType, function::AssociatedFunction},
        syntax::{ArgumentList, Expression, Identifier, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/stream.swift", escape = "none")]
struct StreamTemplate<'a> {
    stream: &'a Stream,
    item: &'a StreamItem,
    pop_batch: &'a Identifier,
    free_buffer: &'a Identifier,
    section: StreamSection,
    indent: &'a str,
    inner_indent: String,
    block_indent: String,
    argument_indent: String,
    read_batch_indent: String,
    body_indent: String,
    item_indent: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamSection {
    Declaration,
    Body,
    ReadBatch,
    Runtime,
    Wire,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    documentation: Documentation,
    name: Identifier,
    owner: Option<TypeName>,
    item: StreamItem,
    delivery: Delivery,
    subscribe: Identifier,
    pop_batch: Identifier,
    wait: Identifier,
    poll: Identifier,
    unsubscribe: Identifier,
    free: Identifier,
    free_buffer: Identifier,
    subscription_binding: Identifier,
    yielded_item_binding: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StreamItem {
    ty: TypeName,
    batch: Batch,
    batch_binding: Identifier,
    batch_count_binding: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Batch {
    Direct {
        element: TypeName,
        expression: Expression,
    },
    Encoded {
        expression: Expression,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectBatch<'a> {
    element: &'a TypeName,
    expression: &'a Expression,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EncodedBatch<'a> {
    expression: &'a Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Delivery {
    Async,
    Batch { subscription: TypeName },
    Callback { cancellable: TypeName },
}

struct StreamItemRenderer<'context, 'bindings> {
    bridge: &'context CBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl Stream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let protocol =
            bridge
                .source_stream(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: SwiftHost::TARGET,
                    invariant: "missing C stream protocol for Swift stream",
                })?;
        Ok(Self {
            documentation: Documentation::new(
                declaration.meta().doc(),
                match declaration.owner() {
                    Some(_) => "    ",
                    None => "",
                },
            ),
            name: Name::new(declaration.name()).function()?,
            owner: declaration
                .owner()
                .map(|owner| SwiftType::class(owner, context))
                .transpose()?,
            item: StreamItem::from_plan(declaration.item(), bridge, context)?,
            delivery: Delivery::new(declaration.mode(), declaration.name())?,
            subscribe: Identifier::parse(protocol.subscribe().name())?,
            pop_batch: Identifier::parse(protocol.pop_batch().name())?,
            wait: Identifier::parse(protocol.wait().name())?,
            poll: Identifier::parse(protocol.poll().name())?,
            unsubscribe: Identifier::parse(protocol.unsubscribe().name())?,
            free: Identifier::parse(protocol.free().name())?,
            free_buffer: Identifier::parse(bridge.support().buffer_free()?.name())?,
            subscription_binding: GeneratedLocal::StreamSubscription.identifier()?,
            yielded_item_binding: Identifier::parse("item")?,
        })
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = StreamTemplate::declaration(self).render()?;
        source.push_str("\n\n");
        let emitted = Emitted::primary(source).with_aux(self.stream_helper()?);
        let emitted = match self.item.requires_wire_runtime() {
            true => emitted
                .with_aux(AssociatedFunction::wire_helper()?)
                .with_aux(self.stream_wire_helper()?),
            false => emitted,
        };
        Ok(emitted)
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn owner(&self) -> Option<&TypeName> {
        self.owner.as_ref()
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn item_type(&self) -> &TypeName {
        &self.item.ty
    }

    fn signature(&self) -> String {
        match &self.delivery {
            Delivery::Async => format!("() -> _Concurrency.AsyncStream<{}>", self.item_type()),
            Delivery::Batch { subscription } => format!("() -> {subscription}"),
            Delivery::Callback { cancellable } => {
                format!(
                    "(callback: @escaping ({}) -> Void) -> {cancellable}",
                    self.item_type()
                )
            }
        }
    }

    fn body(&self, indent: &str) -> String {
        StreamTemplate::body(self, indent).render().unwrap()
    }

    fn async_delivery(&self) -> bool {
        matches!(self.delivery, Delivery::Async)
    }

    fn subscription_binding(&self) -> &Identifier {
        &self.subscription_binding
    }

    fn yielded_item_binding(&self) -> &Identifier {
        &self.yielded_item_binding
    }

    fn poll(&self) -> &Identifier {
        &self.poll
    }

    fn wait(&self) -> &Identifier {
        &self.wait
    }

    fn unsubscribe(&self) -> &Identifier {
        &self.unsubscribe
    }

    fn free(&self) -> &Identifier {
        &self.free
    }

    fn batch_subscription(&self) -> Option<&TypeName> {
        match &self.delivery {
            Delivery::Batch { subscription } => Some(subscription),
            Delivery::Async | Delivery::Callback { .. } => None,
        }
    }

    fn callback_cancellable(&self) -> Option<&TypeName> {
        match &self.delivery {
            Delivery::Callback { cancellable } => Some(cancellable),
            Delivery::Async | Delivery::Batch { .. } => None,
        }
    }

    fn subscribe_call(&self) -> Expression {
        Expression::call(
            &self.subscribe,
            self.owner
                .iter()
                .map(|_| Expression::member("self", "handle"))
                .collect::<ArgumentList>(),
        )
    }

    fn read_batch(&self, indent: &str) -> String {
        StreamTemplate::read_batch(self, indent).render().unwrap()
    }

    fn stream_helper(&self) -> Result<AuxChunk> {
        let mut text = StreamTemplate::runtime(self).render()?;
        text.push_str("\n\n");
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("swift_stream")),
            text: TextChunk::new(text),
        })
    }

    fn stream_wire_helper(&self) -> Result<AuxChunk> {
        let mut text = StreamTemplate::wire(self).render()?;
        text.push_str("\n\n");
        Ok(AuxChunk::Helper {
            id: HelperId::new(CanonicalName::single("swift_wire_stream")),
            text: TextChunk::new(text),
        })
    }
}

impl<'a> StreamTemplate<'a> {
    fn declaration(stream: &'a Stream) -> Self {
        Self::new(StreamSection::Declaration, stream, "")
    }

    fn body(stream: &'a Stream, indent: &'a str) -> Self {
        Self::new(StreamSection::Body, stream, indent)
    }

    fn read_batch(stream: &'a Stream, indent: &'a str) -> Self {
        Self::new(StreamSection::ReadBatch, stream, indent)
    }

    fn runtime(stream: &'a Stream) -> Self {
        Self::new(StreamSection::Runtime, stream, "")
    }

    fn wire(stream: &'a Stream) -> Self {
        Self::new(StreamSection::Wire, stream, "")
    }

    fn new(section: StreamSection, stream: &'a Stream, indent: &'a str) -> Self {
        Self {
            stream,
            item: &stream.item,
            pop_batch: &stream.pop_batch,
            free_buffer: &stream.free_buffer,
            section,
            indent,
            inner_indent: format!("{indent}    "),
            block_indent: format!("{indent}        "),
            argument_indent: format!("{indent}        "),
            read_batch_indent: format!("{indent}            "),
            body_indent: format!("{indent}    "),
            item_indent: format!("{indent}        "),
        }
    }
}

impl StreamSection {
    fn declaration(self) -> bool {
        matches!(self, Self::Declaration)
    }

    fn body(self) -> bool {
        matches!(self, Self::Body)
    }

    fn read_batch(self) -> bool {
        matches!(self, Self::ReadBatch)
    }

    fn runtime(self) -> bool {
        matches!(self, Self::Runtime)
    }

    fn wire(self) -> bool {
        matches!(self, Self::Wire)
    }
}

impl StreamItem {
    fn new(ty: TypeName, batch: Batch) -> Result<Self> {
        Ok(Self {
            ty,
            batch,
            batch_binding: GeneratedLocal::StreamBatch.identifier()?,
            batch_count_binding: GeneratedLocal::StreamBatchCount.identifier()?,
        })
    }

    fn from_plan(
        plan: &StreamItemPlan<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut StreamItemRenderer { bridge, context })
    }

    fn requires_wire_runtime(&self) -> bool {
        matches!(self.batch, Batch::Encoded { .. })
    }

    fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn batch_binding(&self) -> &Identifier {
        &self.batch_binding
    }

    fn batch_count_binding(&self) -> &Identifier {
        &self.batch_count_binding
    }

    fn direct_batch(&self) -> Option<DirectBatch<'_>> {
        match &self.batch {
            Batch::Direct {
                element,
                expression,
            } => Some(DirectBatch {
                element,
                expression,
            }),
            Batch::Encoded { .. } => None,
        }
    }

    fn encoded_batch(&self) -> Option<EncodedBatch<'_>> {
        match &self.batch {
            Batch::Encoded { expression } => Some(EncodedBatch { expression }),
            Batch::Direct { .. } => None,
        }
    }
}

impl DirectBatch<'_> {
    fn element(&self) -> &TypeName {
        self.element
    }

    fn expression(&self) -> &Expression {
        self.expression
    }
}

impl EncodedBatch<'_> {
    fn expression(&self) -> &Expression {
        self.expression
    }
}

impl Batch {
    fn direct_primitive(primitive: Primitive) -> Result<(TypeName, Self)> {
        let ty = SwiftPrimitive::new(primitive).api_type()?;
        Ok((
            ty.clone(),
            Self::Direct {
                element: ty,
                expression: Expression::identifier(Identifier::parse("rawItem")?),
            },
        ))
    }

    fn direct_record(
        record: RecordId,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<(TypeName, Self)> {
        let ty = SwiftType::record(record, context)?;
        let element = bridge
            .source_direct_record(record)
            .map(|record| TypeName::new(record.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing direct record C type for Swift stream",
            })?;
        Ok((
            ty.clone(),
            Self::Direct {
                element,
                expression: Expression::call(
                    ty,
                    [Expression::labeled(
                        "fromC",
                        Expression::identifier(Identifier::parse("rawItem")?),
                    )]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            },
        ))
    }

    fn direct_enum(
        enumeration: boltffi_binding::EnumId,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<(TypeName, Self)> {
        let ty = SwiftType::enumeration(enumeration, context)?;
        let element = bridge
            .source_c_style_enum(enumeration)
            .map(|enumeration| TypeName::new(enumeration.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing C enum type for Swift stream",
            })?;
        Ok((
            ty.clone(),
            Self::Direct {
                element,
                expression: Expression::call(
                    ty,
                    [Expression::labeled(
                        "fromC",
                        Expression::identifier(Identifier::parse("rawItem")?),
                    )]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            },
        ))
    }
}

impl Delivery {
    fn new(mode: StreamMode, name: &CanonicalName) -> Result<Self> {
        let ty = Name::new(name).type_name();
        Ok(match mode {
            StreamMode::Async => Self::Async,
            StreamMode::Batch => Self::Batch {
                subscription: TypeName::new(format!("{ty}Subscription")),
            },
            StreamMode::Callback => Self::Callback {
                cancellable: TypeName::new(format!("{ty}Cancellable")),
            },
            _ => return Err(SwiftHost::unsupported("unknown stream mode")),
        })
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for StreamItemRenderer<'_, '_> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: ByteSize) -> Self::Output {
        let (ty, batch) = match ty {
            DirectValueType::Primitive(primitive) => Batch::direct_primitive(*primitive)?,
            DirectValueType::Record(record) => {
                Batch::direct_record(*record, self.bridge, self.context)?
            }
            DirectValueType::Enum(enumeration) => {
                Batch::direct_enum(*enumeration, self.bridge, self.context)?
            }
            _ => return Err(SwiftHost::unsupported("unknown direct stream item")),
        };
        StreamItem::new(ty, batch)
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        if shape != native::BufferShape::Buffer {
            return Err(SwiftHost::unsupported("encoded stream item shape"));
        }
        let reader = Identifier::parse("reader")?;
        StreamItem::new(
            SwiftType::type_ref(ty, self.context)?,
            Batch::Encoded {
                expression: read
                    .render_with(&mut Reader::new(reader, self.context))
                    .map(ReadExpression::into_expression)?,
            },
        )
    }
}
