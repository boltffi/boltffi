use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeOpaqueFieldExports,
    NativeOpaqueRecordExports, NativeSymbol, Receive, RecordDecl, RecordId, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::{Sizer, WireBuffer},
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            default_value::DefaultExpression,
            field::EncodedField,
            function::{ExportedCall, ExportedCallRenderer, ReceiverCarrier, ReceiverMutation},
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
    opaque_fields: Vec<NativeOpaqueField>,
    initializers: Vec<ExportedCall>,
    static_methods: Vec<ExportedCall>,
    instance_methods: Vec<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordBody {
    Direct { size: u64 },
    Encoded { size: Expression },
    NativeOpaque { exports: NativeOpaqueRecordExports },
}

/// Kotlin field info for a native opaque record field accessor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeOpaqueField {
    pub(crate) name: Identifier,
    pub(crate) ty: TypeName,
    pub(crate) optional: bool,
    pub(crate) has_fn: Option<Identifier>,
    pub(crate) access: NativeOpaqueFieldKind,
    /// For unsigned primitives, the conversion method to apply (e.g. `.toUInt()`).
    pub(crate) conversion: Option<Identifier>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeOpaqueFieldKind {
    Primitive {
        get_fn: Identifier,
    },
    Borrow {
        borrow_fn: Identifier,
        is_string: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Receiver {
    carrier: ReceiverCarrier,
    mutation: ReceiverMutation,
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
            RecordDecl::Encoded(record) if record.is_native_opaque() => {
                Self::from_native_opaque(record)
            }
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
            RecordBody::Encoded { .. } | RecordBody::NativeOpaque { .. } => 0,
        }
    }

    pub fn wire_size(&self) -> Option<&Expression> {
        match &self.body {
            RecordBody::Encoded { size } => Some(size),
            RecordBody::Direct { .. } | RecordBody::NativeOpaque { .. } => None,
        }
    }

    pub fn encoded(&self) -> bool {
        matches!(self.body, RecordBody::Encoded { .. })
    }

    pub fn native_opaque(&self) -> bool {
        matches!(self.body, RecordBody::NativeOpaque { .. })
    }

    pub fn native_opaque_drop_fn(&self) -> Option<Identifier> {
        match &self.body {
            RecordBody::NativeOpaque { exports } => {
                Identifier::parse(exports.drop().name().as_str()).ok()
            }
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn native_opaque_exports(&self) -> Option<&NativeOpaqueRecordExports> {
        match &self.body {
            RecordBody::NativeOpaque { exports } => Some(exports),
            _ => None,
        }
    }

    pub fn opaque_fields(&self) -> &[NativeOpaqueField] {
        &self.opaque_fields
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
            RecordBody::Encoded { .. } | RecordBody::NativeOpaque { .. } => &[],
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

    pub fn direct_buffer_expression(value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            value,
            Identifier::parse("toDirectBuffer")?,
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

    fn from_native_opaque(record: &EncodedRecordDecl<Native>) -> Result<Self> {
        let exports = record
            .native_opaque_exports()
            .ok_or(KotlinHost::broken_bridge_contract(
                "native opaque record missing helper exports",
            ))?;
        let opaque_fields = record
            .fields()
            .iter()
            .zip(exports.fields())
            .map(|(field, fexp)| NativeOpaqueField::from_field(field, fexp))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::NativeOpaque {
                exports: exports.clone(),
            },
            error: false,
            fields: Vec::new(),
            opaque_fields,
            initializers: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
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
                .map(|field| Field::from_direct(field, record, &buffer, context))
                .collect::<Result<Vec<_>>>()?,
            opaque_fields: Vec::new(),
            initializers: Self::initializer_calls(record.initializers(), host, bridge, context)?,
            static_methods: Self::methods(record.methods(), None, host, bridge, context)?,
            instance_methods: Self::methods(
                record.methods(),
                Some(Self::direct_receiver(record.name())?),
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
            opaque_fields: Vec::new(),
            initializers: Self::initializer_calls(record.initializers(), host, bridge, context)?,
            static_methods: Self::methods(record.methods(), None, host, bridge, context)?,
            instance_methods: Self::methods(
                record.methods(),
                Some(Self::encoded_receiver(record.name())?),
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
                    None,
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
                    (Some(Receive::ByMutRef), Some(receiver)) => calls.with_receiver_mutation(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        receiver.carrier,
                        receiver.mutation,
                    ),
                    (Some(Receive::ByRef | Receive::ByValue), Some(receiver)) => calls.exported(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        Some(receiver.carrier),
                    ),
                    (None, None) => calls.exported(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        None,
                    ),
                    _ => Err(KotlinHost::unsupported("record method receiver")),
                },
            )
            .collect()
    }

    fn direct_receiver(name: &CanonicalName) -> Result<Receiver> {
        let buffer = Identifier::parse("__boltffi_receiver")?;
        Ok(Receiver {
            carrier: ReceiverCarrier::direct_writeback(
                buffer.clone(),
                Self::direct_buffer_expression(Expression::this())?,
            ),
            mutation: ReceiverMutation::direct(Name::new(name).type_name(), buffer),
        })
    }

    fn encoded_receiver(name: &CanonicalName) -> Result<Receiver> {
        let buffer = WireBuffer::new(&Name::new(name))?;
        let writer = buffer.writer().clone();
        let write = buffer.write_statements(
            Expression::call(
                Expression::this(),
                Identifier::parse("wireSize")?,
                ArgumentList::default(),
            ),
            vec![Statement::expression(Expression::call(
                Expression::this(),
                Identifier::parse("writeTo")?,
                [Expression::identifier(writer)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))],
        )?;
        Ok(Receiver {
            carrier: ReceiverCarrier::encoded(write),
            mutation: ReceiverMutation::encoded(Name::new(name).type_name()),
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
        context: &RenderContext<Native>,
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
            .map(|value| DefaultExpression::render(&TypeRef::Primitive(primitive), value, context))
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
            .map(|value| DefaultExpression::render(field.ty(), value, context))
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

impl NativeOpaqueField {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn optional(&self) -> bool {
        self.optional
    }

    pub fn has_fn(&self) -> Option<&Identifier> {
        self.has_fn.as_ref()
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self.access, NativeOpaqueFieldKind::Primitive { .. })
    }

    pub fn is_string(&self) -> bool {
        matches!(&self.access, NativeOpaqueFieldKind::Borrow { is_string, .. } if *is_string)
    }

    #[allow(dead_code)]
    pub fn is_bytes(&self) -> bool {
        matches!(&self.access, NativeOpaqueFieldKind::Borrow { is_string, .. } if !is_string)
    }

    pub fn get_fn(&self) -> Option<&Identifier> {
        match &self.access {
            NativeOpaqueFieldKind::Primitive { get_fn } => Some(get_fn),
            _ => None,
        }
    }

    pub fn borrow_fn(&self) -> Option<&Identifier> {
        match &self.access {
            NativeOpaqueFieldKind::Borrow { borrow_fn, .. } => Some(borrow_fn),
            _ => None,
        }
    }

    fn from_field(field: &EncodedFieldDecl, exports: &NativeOpaqueFieldExports) -> Result<Self> {
        use crate::target::kotlin::primitive::KotlinPrimitive;
        let key = field.key();
        let name = match key {
            FieldKey::Named(n) => Name::new(n).parameter()?,
            _ => return Err(KotlinHost::unsupported("unnamed native opaque field")),
        };
        let (ty_inner, optional) = match field.ty() {
            TypeRef::Optional(inner) => (inner.as_ref(), true),
            ty => (ty, false),
        };
        let has_fn = if optional {
            Some(Identifier::parse(
                exports
                    .has()
                    .ok_or(KotlinHost::broken_bridge_contract(
                        "optional native opaque field without has export",
                    ))?
                    .name()
                    .as_str(),
            )?)
        } else {
            None
        };
        let (ty, access) = match ty_inner {
            TypeRef::Primitive(p) => {
                let get_sym = exports
                    .get()
                    .ok_or(KotlinHost::broken_bridge_contract(
                        "primitive native opaque field without get export",
                    ))?
                    .name()
                    .as_str();
                let kotlin_ty = KotlinPrimitive::new(*p).api_type()?;
                let ty = if optional {
                    kotlin_ty.nullable()
                } else {
                    kotlin_ty
                };
                (
                    ty,
                    NativeOpaqueFieldKind::Primitive {
                        get_fn: Identifier::parse(get_sym)?,
                    },
                )
            }
            TypeRef::String => {
                let borrow_sym = exports
                    .borrow()
                    .ok_or(KotlinHost::broken_bridge_contract(
                        "string native opaque field without borrow export",
                    ))?
                    .name()
                    .as_str();
                let ty = if optional {
                    TypeName::new("String").nullable()
                } else {
                    TypeName::new("String")
                };
                (
                    ty,
                    NativeOpaqueFieldKind::Borrow {
                        borrow_fn: Identifier::parse(borrow_sym)?,
                        is_string: true,
                    },
                )
            }
            TypeRef::Bytes => {
                let borrow_sym = exports
                    .borrow()
                    .ok_or(KotlinHost::broken_bridge_contract(
                        "bytes native opaque field without borrow export",
                    ))?
                    .name()
                    .as_str();
                let ty = if optional {
                    TypeName::new("ByteArray").nullable()
                } else {
                    TypeName::new("ByteArray")
                };
                (
                    ty,
                    NativeOpaqueFieldKind::Borrow {
                        borrow_fn: Identifier::parse(borrow_sym)?,
                        is_string: false,
                    },
                )
            }
            _ => {
                return Err(KotlinHost::unsupported(
                    "unsupported native opaque field type",
                ));
            }
        };
        // Compute unsigned conversion for the public API type
        let conversion = match ty_inner {
            TypeRef::Primitive(boltffi_binding::Primitive::U8) => {
                Some(Identifier::parse("toUByte")?)
            }
            TypeRef::Primitive(boltffi_binding::Primitive::U16) => {
                Some(Identifier::parse("toUShort")?)
            }
            TypeRef::Primitive(boltffi_binding::Primitive::U32) => {
                Some(Identifier::parse("toUInt")?)
            }
            TypeRef::Primitive(boltffi_binding::Primitive::U64)
            | TypeRef::Primitive(boltffi_binding::Primitive::USize) => {
                Some(Identifier::parse("toULong")?)
            }
            _ => None,
        };
        Ok(Self {
            name,
            ty,
            optional,
            has_fn,
            access,
            conversion,
        })
    }
}

impl NativeOpaqueField {
    pub fn conversion(&self) -> Option<&Identifier> {
        self.conversion.as_ref()
    }
}
