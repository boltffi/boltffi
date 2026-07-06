use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeSymbol, Receive, RecordDecl,
    RecordId, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::Sizer,
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            default_value::DefaultExpression,
            field::EncodedField,
            function::{EncodedReceiverMutation, ExportedCall, ExportedCallRenderer},
        },
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/record.kt", escape = "none")]
struct RecordTemplate {
    record: Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    name: TypeName,
    body: RecordBody,
    error: bool,
    fields: Vec<Field>,
    initializers: Vec<ExportedCall>,
    static_methods: Vec<ExportedCall>,
    instance_methods: Vec<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordBody {
    Direct { size: u64 },
    Encoded { size: Expression },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Receiver {
    argument: Expression,
    writeback: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    name: Identifier,
    ty: TypeName,
    read: Expression,
    read_from_base: Option<Expression>,
    write: Statement,
    write_from_base: Option<Statement>,
    size: Option<Expression>,
    default: Option<Expression>,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, host, bridge, context),
            RecordDecl::Encoded(record) => Self::from_encoded(record, host, bridge, context),
            _ => Err(KotlinHost::unsupported("unknown record declaration")),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(RecordTemplate { record: self }.render()?))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn size(&self) -> u64 {
        match self.body {
            RecordBody::Direct { size } => size,
            RecordBody::Encoded { .. } => 0,
        }
    }

    pub fn wire_size(&self) -> Option<&Expression> {
        match &self.body {
            RecordBody::Encoded { size } => Some(size),
            RecordBody::Direct { .. } => None,
        }
    }

    pub fn encoded(&self) -> bool {
        matches!(self.body, RecordBody::Encoded { .. })
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn error_message(&self) -> Option<&Identifier> {
        self.fields
            .iter()
            .find(|field| field.is_string_message())
            .map(|field| field.name())
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn initializers(&self) -> &[ExportedCall] {
        &self.initializers
    }

    pub fn static_methods(&self) -> &[ExportedCall] {
        &self.static_methods
    }

    pub fn instance_methods(&self) -> &[ExportedCall] {
        &self.instance_methods
    }

    pub fn direct_fields(&self) -> &[Field] {
        match self.body {
            RecordBody::Direct { .. } => &self.fields,
            RecordBody::Encoded { .. } => &[],
        }
    }

    pub fn empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn type_name_from_id(id: RecordId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .record(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "record type was not found in render context",
            ))
            .map(Self::name_from_declaration)
    }

    pub fn direct_size_from_id(id: RecordId, context: &RenderContext<Native>) -> Result<u64> {
        context
            .record(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "record type was not found in render context",
            ))
            .and_then(|record| match record {
                RecordDecl::Direct(record) => Ok(record.layout().size().get()),
                RecordDecl::Encoded(_) => Err(KotlinHost::broken_bridge_contract(
                    "direct-vector record was not lowered as a direct record",
                )),
                _ => Err(KotlinHost::unsupported("unknown record declaration")),
            })
    }

    pub fn encode_expression(value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            value,
            Identifier::parse("toByteArray")?,
            Default::default(),
        ))
    }

    pub fn decode_expression(record: TypeName, value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            record,
            Identifier::parse("fromByteArray")?,
            [value].into_iter().collect(),
        ))
    }

    fn from_direct(
        record: &DirectRecordDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let buffer = Identifier::parse("buffer")?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::Direct {
                size: record.layout().size().get(),
            },
            error: record.is_error_payload(),
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_direct(field, record, &buffer))
                .collect::<Result<Vec<_>>>()?,
            initializers: Self::initializer_calls(record.initializers(), host, bridge, context)?,
            static_methods: Self::methods(record.methods(), None, host, bridge, context)?,
            instance_methods: Self::methods(
                record.methods(),
                Some(Self::receiver(record.name())?),
                host,
                bridge,
                context,
            )?,
        })
    }

    fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let current = Expression::this();
        let size = record
            .fields()
            .iter()
            .map(|field| {
                field
                    .write()
                    .size_with(&mut Sizer::new(host, context)?.current(current.clone()))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|size| size.into_expression())
            .reduce(Expression::add)
            .unwrap_or_else(|| Expression::integer(0));
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::Encoded { size },
            error: record.is_error_payload(),
            fields: record
                .fields()
                .iter()
                .map(|field| {
                    Field::from_encoded(field, host, context, &reader, &writer, current.clone())
                })
                .collect::<Result<Vec<_>>>()?,
            initializers: Self::initializer_calls(record.initializers(), host, bridge, context)?,
            static_methods: Self::methods(record.methods(), None, host, bridge, context)?,
            instance_methods: Self::methods(
                record.methods(),
                Some(Self::receiver(record.name())?),
                host,
                bridge,
                context,
            )?,
        })
    }

    fn name_from_declaration(record: &RecordDecl<Native>) -> TypeName {
        Name::new(record.name()).type_name()
    }

    fn initializer_calls(
        initializers: &[InitializerDecl<Native>],
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<ExportedCall>> {
        let calls = ExportedCallRenderer::new(host, bridge, context);
        initializers
            .iter()
            .map(|initializer| {
                calls.exported(
                    Name::new(initializer.name()).function()?,
                    initializer.symbol(),
                    initializer.callable(),
                    Vec::new(),
                )
            })
            .collect()
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<ExportedCall>> {
        let calls = ExportedCallRenderer::new(host, bridge, context);
        methods
            .iter()
            .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
            .map(
                |method| match (method.callable().receiver(), receiver.clone()) {
                    (Some(Receive::ByMutRef), Some(receiver)) => calls
                        .with_encoded_receiver_mutation(
                            Name::new(method.name()).function()?,
                            method.target(),
                            method.callable(),
                            vec![receiver.argument],
                            EncodedReceiverMutation::new(receiver.writeback),
                        ),
                    (Some(Receive::ByRef | Receive::ByValue), Some(receiver)) => calls.exported(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        vec![receiver.argument],
                    ),
                    (None, None) => calls.exported(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        Vec::new(),
                    ),
                    _ => Err(KotlinHost::unsupported("record method receiver")),
                },
            )
            .collect()
    }

    fn receiver(name: &CanonicalName) -> Result<Receiver> {
        Ok(Receiver {
            argument: Expression::call(
                Expression::this(),
                Identifier::parse("toByteArray")?,
                ArgumentList::default(),
            ),
            writeback: Name::new(name).type_name(),
        })
    }
}

impl Field {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn is_string_message(&self) -> bool {
        self.name.to_string() == "message" && self.ty.to_string() == "String"
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn read(&self) -> &Expression {
        &self.read
    }

    pub fn read_from_base(&self) -> &Expression {
        self.read_from_base
            .as_ref()
            .expect("direct field has offset-based read expression")
    }

    pub fn write(&self) -> &Statement {
        &self.write
    }

    pub fn write_from_base(&self) -> &Statement {
        self.write_from_base
            .as_ref()
            .expect("direct field has offset-based write expression")
    }

    pub fn default(&self) -> Option<&Expression> {
        self.default.as_ref()
    }

    fn from_direct(
        field: &DirectFieldDecl,
        record: &DirectRecordDecl<Native>,
        buffer: &Identifier,
    ) -> Result<Self> {
        let name = Self::identifier(field.key())?;
        let offset = record
            .layout()
            .field(field.key())
            .ok_or(KotlinHost::broken_bridge_contract(
                "direct record field layout was not found",
            ))?
            .offset()
            .get();
        let base = Identifier::parse("offset")?;
        let position = match offset {
            0 => Expression::identifier(base),
            _ => Expression::identifier(base).add(Expression::integer(offset)),
        };
        let primitive = field.ty().primitive();
        let default = field
            .meta()
            .default()
            .map(|value| DefaultExpression::render(&TypeRef::Primitive(primitive), value))
            .transpose()?;
        Ok(Self {
            ty: KotlinPrimitive::new(primitive).api_type()?,
            read: KotlinPrimitive::new(primitive).buffer_read(buffer, offset)?,
            read_from_base: Some(
                KotlinPrimitive::new(primitive).buffer_read_at(buffer, position.clone())?,
            ),
            write: KotlinPrimitive::new(primitive).buffer_write(
                buffer,
                offset,
                Expression::identifier(name.clone()),
            )?,
            write_from_base: Some(KotlinPrimitive::new(primitive).buffer_write_at(
                buffer,
                position,
                Expression::identifier(name.clone()),
            )?),
            size: None,
            default,
            name,
        })
    }

    fn from_encoded(
        field: &EncodedFieldDecl,
        host: &KotlinHost,
        context: &RenderContext<Native>,
        reader: &Identifier,
        writer: &Identifier,
        current: Expression,
    ) -> Result<Self> {
        let name = Self::identifier(field.key())?;
        let default = field
            .meta()
            .default()
            .map(|value| DefaultExpression::render(field.ty(), value))
            .transpose()?;
        let field = EncodedField::from_declaration(field, host, context, reader, writer, current)?;
        Ok(Self {
            ty: field.ty().clone(),
            read: field.read().clone(),
            read_from_base: None,
            write: field.write().clone(),
            write_from_base: None,
            size: Some(field.size().clone()),
            default,
            name,
        })
    }

    fn identifier(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).parameter(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(KotlinHost::unsupported("unknown direct record field key")),
        }
    }
}
