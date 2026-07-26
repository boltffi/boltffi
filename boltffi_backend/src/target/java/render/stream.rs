use boltffi_binding::{
    ByteSize, ClassId, DeclarationRef, DirectValueType, Native, Primitive as BindingPrimitive,
    ReadPlan, StreamDecl, StreamItemPlan, StreamItemPlanRender, StreamMode, TypeRef, native,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{AuxChunk, RenderContext, Result},
    target::java::{
        JavaHost, JavaVersion,
        codec::{Reader, Runtime},
        name_style::Name,
        primitive::Primitive,
        render::{
            DirectVector, Enumeration, Record,
            native::Method,
            signature::{ErasedSignature, ReturnType, ValueType},
            type_name::JavaType,
        },
        syntax::{
            ArgumentList, Expression, Identifier, Javadoc, Statement, TypeIdentifier, TypeName,
        },
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    name: Identifier,
    item: StreamItem,
    delivery: Delivery,
    subscribe: Expression,
    pop_batch: Expression,
    wait: Expression,
    poll: Expression,
    unsubscribe: Expression,
    free: Expression,
    native_methods: Vec<Method>,
    doc: Option<Javadoc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Delivery {
    Callback,
    Batch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StreamItem {
    ty: TypeName,
    setup: Vec<Statement>,
    items: Expression,
    runtime: ItemRuntime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemRuntime {
    Direct,
    Wire,
}

struct StreamItemRenderer<'context> {
    version: JavaVersion,
    context: &'context RenderContext<'context, Native>,
}

impl Stream {
    pub fn for_class(
        owner: ClassId,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Self>> {
        context
            .bindings()
            .decls()
            .iter()
            .filter_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Stream(stream) if stream.owner() == Some(owner) => Some(stream),
                _ => None,
            })
            .map(|stream| Self::from_declaration(stream, bridge, native_owner, version, context))
            .collect()
    }

    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Primitive::from_handle_carrier(declaration.handle())?;
        let protocol = declaration.protocol();
        let subscribe = Method::from_symbol(protocol.subscribe(), bridge, version)?;
        let pop_batch = match bridge.source_direct_batch(protocol.pop_batch().id()) {
            Some(method) => Method::from_direct_stream_batch(method, version)?,
            None => Method::from_symbol(protocol.pop_batch(), bridge, version)?,
        };
        let wait = Method::from_symbol(protocol.wait(), bridge, version)?;
        let poll = Method::from_symbol(protocol.poll(), bridge, version)?;
        let unsubscribe = Method::from_symbol(protocol.unsubscribe(), bridge, version)?;
        let free = Method::from_symbol(protocol.free(), bridge, version)?;
        subscribe.validate_return(&ReturnType::Value(ValueType::Primitive(Primitive::Long)))?;
        pop_batch.validate_return(&ReturnType::Value(ValueType::Reference(TypeName::array(
            TypeName::primitive(Primitive::Byte),
        ))))?;
        wait.validate_return(&ReturnType::Value(ValueType::Primitive(Primitive::Int)))?;
        poll.validate_return(&ReturnType::Void)?;
        unsubscribe.validate_return(&ReturnType::Void)?;
        free.validate_return(&ReturnType::Void)?;
        let receiver =
            Expression::this().call(Identifier::known("rawHandle"), ArgumentList::default());
        let subscription = Expression::identifier(Identifier::known("streamHandle"));
        let max_count = Expression::identifier(Identifier::known("maxCount"));
        let continuation = Expression::identifier(Identifier::known("continuation"));
        Ok(Self {
            name: Name::new(declaration.name()).function(version)?,
            item: StreamItem::from_plan(declaration.item(), version, context)?,
            delivery: Delivery::from_mode(declaration.mode())?,
            subscribe: subscribe.call(native_owner, [receiver])?,
            pop_batch: pop_batch.call(native_owner, [subscription.clone(), max_count])?,
            wait: wait.call(
                native_owner,
                [
                    subscription.clone(),
                    Expression::identifier(Identifier::known("timeout")),
                ],
            )?,
            poll: poll.call(native_owner, [subscription.clone(), continuation])?,
            unsubscribe: unsubscribe.call(native_owner, [subscription.clone()])?,
            free: free.call(native_owner, [subscription])?,
            native_methods: vec![subscribe, pop_batch, wait, poll, unsubscribe, free],
            doc: declaration.meta().doc().map(Javadoc::new),
        })
    }

    pub fn native_forwards(&self) -> Result<Vec<AuxChunk>> {
        self.native_methods
            .iter()
            .map(|method| method.render().map(Into::into).map(AuxChunk::ForwardDecl))
            .chain(std::iter::once(Runtime::async_callback()))
            .collect()
    }

    pub fn runtime_helpers(&self, version: JavaVersion) -> Result<Vec<AuxChunk>> {
        Ok([
            Some(Runtime::stream_helper(version.supports_flow_api())?),
            Some(Runtime::async_helper()?),
            matches!(self.item.runtime, ItemRuntime::Direct)
                .then(Runtime::direct_vector_helper)
                .transpose()?,
            matches!(self.item.runtime, ItemRuntime::Wire)
                .then(Runtime::helper)
                .transpose()?,
        ]
        .into_iter()
        .flatten()
        .collect())
    }

    pub fn signature(&self, version: JavaVersion) -> ErasedSignature {
        let parameters = match self.delivery {
            Delivery::Callback => vec![ValueType::Reference(TypeName::parameterized(
                TypeName::qualified(
                    ["java", "util", "function"]
                        .into_iter()
                        .map(Identifier::known)
                        .collect(),
                    TypeIdentifier::known("Consumer", version),
                ),
                [self.item.ty.clone()],
            ))],
            Delivery::Batch => Vec::new(),
        };
        ErasedSignature::new(self.name.clone(), parameters)
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn item_type(&self) -> &TypeName {
        &self.item.ty
    }

    pub fn callback_delivery(&self) -> bool {
        self.delivery == Delivery::Callback
    }

    pub fn subscribe(&self) -> &Expression {
        &self.subscribe
    }

    pub fn pop_batch(&self) -> &Expression {
        &self.pop_batch
    }

    pub fn wait(&self) -> &Expression {
        &self.wait
    }

    pub fn poll(&self) -> &Expression {
        &self.poll
    }

    pub fn unsubscribe(&self) -> &Expression {
        &self.unsubscribe
    }

    pub fn free(&self) -> &Expression {
        &self.free
    }

    pub fn item_setup(&self) -> &[Statement] {
        &self.item.setup
    }

    pub fn items(&self) -> &Expression {
        &self.item.items
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }
}

impl Delivery {
    fn from_mode(mode: StreamMode) -> Result<Self> {
        match mode {
            StreamMode::Async | StreamMode::Callback => Ok(Self::Callback),
            StreamMode::Batch => Ok(Self::Batch),
            _ => Err(JavaHost::unsupported("unknown stream mode")),
        }
    }
}

impl StreamItem {
    fn from_plan(
        plan: &StreamItemPlan<Native>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut StreamItemRenderer { version, context })
    }
}

impl StreamItemRenderer<'_> {
    fn direct_primitive(&self, source: BindingPrimitive, size: ByteSize) -> Result<StreamItem> {
        let primitive = Primitive::try_from(source)?;
        if primitive.wire_size() != size.get() {
            return Err(JavaHost::broken_bridge_contract(
                "Java direct stream primitive size matches the binding contract",
            ));
        }
        Ok(StreamItem {
            ty: TypeName::boxed_primitive(primitive, self.version),
            setup: Vec::new(),
            items: Self::batch_call(
                match primitive {
                    Primitive::Boolean => "booleans",
                    Primitive::Byte => "bytes",
                    Primitive::Short => "shorts",
                    Primitive::Int => "ints",
                    Primitive::Long => "longs",
                    Primitive::Float => "floats",
                    Primitive::Double => "doubles",
                },
                [Self::bytes()],
                self.version,
            ),
            runtime: ItemRuntime::Direct,
        })
    }

    fn direct_record(
        &self,
        record: boltffi_binding::RecordId,
        size: ByteSize,
    ) -> Result<StreamItem> {
        if Record::direct_size_for(record, self.context)? != size.get() {
            return Err(JavaHost::broken_bridge_contract(
                "Java direct stream record size matches the binding contract",
            ));
        }
        let vector = DirectVector::from_element(
            &boltffi_binding::DirectVectorElementType::record(record),
            self.version,
            self.context,
        )?;
        Ok(StreamItem {
            ty: TypeName::named(Record::type_name_for(record, self.context, self.version)?),
            setup: Vec::new(),
            items: vector.returned_expression(Self::bytes()),
            runtime: ItemRuntime::Direct,
        })
    }

    fn direct_enum(
        &self,
        enumeration: boltffi_binding::EnumId,
        size: ByteSize,
    ) -> Result<StreamItem> {
        let primitive = Enumeration::c_style_primitive(enumeration, self.context)?;
        if primitive.wire_size() != size.get() {
            return Err(JavaHost::broken_bridge_contract(
                "Java direct stream enum size matches the binding contract",
            ));
        }
        let value = Identifier::known("value");
        let raw = Self::batch_call(
            match primitive {
                Primitive::Boolean => "booleans",
                Primitive::Byte => "bytes",
                Primitive::Short => "shorts",
                Primitive::Int => "ints",
                Primitive::Long => "longs",
                Primitive::Float | Primitive::Double => {
                    return Err(JavaHost::broken_bridge_contract(
                        "Java direct stream enum has an integer representation",
                    ));
                }
            },
            [Self::bytes()],
            self.version,
        );
        let ty = TypeName::named(Enumeration::type_name_for(
            enumeration,
            self.context,
            self.version,
        )?);
        Ok(StreamItem {
            ty: ty.clone(),
            setup: Vec::new(),
            items: Self::batch_call(
                "map",
                [
                    raw,
                    Expression::lambda(
                        [value.clone()],
                        Expression::static_call(
                            ty,
                            Identifier::known("fromValue"),
                            [Expression::identifier(value)].into_iter().collect(),
                        ),
                    ),
                ],
                self.version,
            ),
            runtime: ItemRuntime::Direct,
        })
    }

    fn batch_call(
        method: &'static str,
        arguments: impl IntoIterator<Item = Expression>,
        version: JavaVersion,
    ) -> Expression {
        Expression::static_call(
            TypeName::named(TypeIdentifier::known("BoltFfiStreamBatches", version)),
            Identifier::known(method),
            arguments.into_iter().collect::<ArgumentList>(),
        )
    }

    fn bytes() -> Expression {
        Expression::identifier(Identifier::known("bytes"))
    }
}

impl<'plan> StreamItemPlanRender<'plan, Native> for StreamItemRenderer<'_> {
    type Output = Result<StreamItem>;

    fn direct(&mut self, ty: &'plan DirectValueType, size: ByteSize) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => self.direct_primitive(*primitive, size),
            DirectValueType::Record(record) => self.direct_record(*record, size),
            DirectValueType::Enum(enumeration) => self.direct_enum(*enumeration, size),
            _ => Err(JavaHost::unsupported("unknown direct stream item")),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        if shape != native::BufferShape::Buffer {
            return Err(JavaHost::unsupported("encoded stream item shape"));
        }
        let reader = Identifier::known("reader");
        let item = read
            .render_with(&mut Reader::new(reader.clone(), self.version, self.context))?
            .into_expression();
        Ok(StreamItem {
            ty: JavaType::boxed_type_ref(ty, self.version, self.context)?,
            setup: vec![Statement::value(
                TypeName::named(TypeIdentifier::known("WireReader", self.version)),
                reader.clone(),
                Expression::construct(
                    TypeName::named(TypeIdentifier::known("WireReader", self.version)),
                    [Self::bytes()].into_iter().collect(),
                ),
            )],
            items: Expression::identifier(reader).call(
                Identifier::known("readSequence"),
                [Expression::lambda([], item)].into_iter().collect(),
            ),
            runtime: ItemRuntime::Wire,
        })
    }
}
