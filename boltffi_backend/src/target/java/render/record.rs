use askama::Template as AskamaTemplate;
use boltffi_binding::{
    DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, Native, RecordDecl,
    RecordId,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::java::{
        JavaFile, JavaHost, JavaPackage, JavaVersion,
        admission::RecordShape,
        codec::{Reader, Runtime, Sizer, ValueMemberAccess, Writer},
        name_style::Name,
        primitive::Primitive,
        render::{
            ValueIdentity,
            call::{AssociatedCallContext, Call, ValueCalls, ValueReceiver},
            default_value::DefaultExpression,
            signature::Parameter,
            type_name::JavaType,
        },
        syntax::{
            ArgumentList, Expression, Identifier, Javadoc, Statement, TypeIdentifier, TypeName,
        },
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/record.java", escape = "none")]
struct RecordTemplate<'record> {
    package: &'record JavaPackage,
    record: &'record Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    Direct { size: u64, codec_payload: bool },
    Encoded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FieldStorage {
    Direct { primitive: Primitive, offset: u64 },
    Encoded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Form {
    Class,
    Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    name: TypeIdentifier,
    form: Form,
    body: Body,
    error: bool,
    fields: Vec<Field>,
    default_constructors: Vec<DefaultConstructor>,
    initializers: Vec<Call>,
    static_methods: Vec<Call>,
    instance_methods: Vec<Call>,
    wire_size: Expression,
    error_message: Option<Identifier>,
    doc: Option<Javadoc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    name: Identifier,
    ty: TypeName,
    storage: FieldStorage,
    wire_read: Expression,
    wire_write: Vec<Statement>,
    wire_size: Expression,
    equals: Expression,
    hash: Expression,
    default: Option<Expression>,
    doc: Option<Javadoc>,
    native_record_safe: bool,
    requires_identity: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultConstructor {
    parameters: Vec<Parameter<TypeName>>,
    arguments: ArgumentList,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        RecordShape::classify(declaration).require_supported()?;
        match declaration {
            RecordDecl::Direct(record) => {
                Self::from_direct(record, bridge, native_owner, version, context)
            }
            RecordDecl::Encoded(record) => {
                Self::from_encoded(record, bridge, native_owner, version, context)
            }
            _ => Err(JavaHost::unsupported("unknown record declaration")),
        }
    }

    pub fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        let emitted = Emitted::primary(
            RecordTemplate {
                package,
                record: self,
            }
            .render()?,
        );
        let emitted = match self.fields.iter().any(Field::requires_identity) {
            true => emitted.with_aux(ValueIdentity::helper()?),
            false => emitted,
        };
        let emitted = match self.requires_wire_runtime() {
            true => emitted.with_aux(Runtime::helper()?),
            false => emitted,
        };
        let emitted = match self
            .initializers
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
            .any(Call::requires_direct_vector_runtime)
        {
            true => emitted.with_aux(Runtime::direct_vector_helper()?),
            false => emitted,
        };
        let emitted = match self
            .initializers
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
            .any(Call::requires_async_runtime)
        {
            true => emitted.with_aux(Runtime::async_helper()?),
            false => emitted,
        };
        self.initializers
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
            .try_fold(emitted, |emitted, call| {
                Ok(call
                    .native_forwards()?
                    .into_iter()
                    .fold(emitted, Emitted::with_aux))
            })
    }

    pub fn file_for(declaration: &RecordDecl<Native>, version: JavaVersion) -> Result<JavaFile> {
        Name::new(declaration.name())
            .type_name(version)
            .and_then(|name| JavaFile::parse_for(name.as_str(), version))
    }

    pub fn type_name_for(
        id: RecordId,
        context: &RenderContext<Native>,
        version: JavaVersion,
    ) -> Result<TypeIdentifier> {
        context
            .record(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "record type was not found in render context",
            ))
            .and_then(|record| Name::new(record.name()).type_name(version))
    }

    pub fn direct_size_for(id: RecordId, context: &RenderContext<Native>) -> Result<u64> {
        match context.record(id) {
            Some(RecordDecl::Direct(record)) => Ok(record.layout().size().get()),
            Some(_) => Err(JavaHost::broken_bridge_contract(
                "direct vector record has an encoded declaration",
            )),
            None => Err(JavaHost::broken_bridge_contract(
                "direct vector record was not found in render context",
            )),
        }
    }

    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn initializers(&self) -> &[Call] {
        &self.initializers
    }

    pub fn default_constructors(&self) -> &[DefaultConstructor] {
        &self.default_constructors
    }

    pub fn static_methods(&self) -> &[Call] {
        &self.static_methods
    }

    pub fn instance_methods(&self) -> &[Call] {
        &self.instance_methods
    }

    pub fn size(&self) -> u64 {
        match self.body {
            Body::Direct { size, .. } => size,
            Body::Encoded => 0,
        }
    }

    pub fn direct(&self) -> bool {
        matches!(self.body, Body::Direct { .. })
    }

    pub fn codec_payload(&self) -> bool {
        match self.body {
            Body::Direct { codec_payload, .. } => codec_payload,
            Body::Encoded => true,
        }
    }

    pub fn native_record(&self) -> bool {
        matches!(self.form, Form::Record)
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn error_message(&self) -> Option<&Identifier> {
        self.error_message.as_ref()
    }

    pub fn empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn wire_size(&self) -> &Expression {
        &self.wire_size
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    fn from_direct(
        record: &DirectRecordDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(record.name()).type_name(version)?;
        let fields = record
            .fields()
            .iter()
            .map(|field| Field::from_direct(field, record, version))
            .collect::<Result<Vec<_>>>()?;
        let (initializers, static_methods, instance_methods) = ValueCalls::from_declarations(
            record.initializers(),
            record.methods(),
            ValueReceiver::DirectRecord(name.clone()),
            AssociatedCallContext::local(bridge, native_owner, version, context),
        )?
        .into_parts();
        let default_constructors = DefaultConstructor::from_fields(&fields);
        let error = record.is_error_payload();
        Ok(Self {
            name,
            form: match version.supports_records()
                && !error
                && fields.iter().all(Field::native_record_safe)
            {
                true => Form::Record,
                false => Form::Class,
            },
            body: Body::Direct {
                size: record.layout().size().get(),
                codec_payload: record.is_codec_payload(),
            },
            error,
            wire_size: Self::sum_sizes(&fields),
            error_message: error.then(|| Self::error_message_field(&fields)).flatten(),
            fields,
            default_constructors,
            initializers,
            static_methods,
            instance_methods,
            doc: record.meta().doc().map(Javadoc::new),
        })
    }

    fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(record.name()).type_name(version)?;
        let fields = record
            .fields()
            .iter()
            .map(|field| Field::from_encoded(field, version, context))
            .collect::<Result<Vec<_>>>()?;
        let (initializers, static_methods, instance_methods) = ValueCalls::from_declarations(
            record.initializers(),
            record.methods(),
            ValueReceiver::Encoded {
                ty: name.clone(),
                codec: record.write().clone(),
            },
            AssociatedCallContext::local(bridge, native_owner, version, context),
        )?
        .into_parts();
        let default_constructors = DefaultConstructor::from_fields(&fields);
        let error = record.is_error_payload();
        Ok(Self {
            name,
            form: match version.supports_records()
                && !error
                && fields.iter().all(Field::native_record_safe)
            {
                true => Form::Record,
                false => Form::Class,
            },
            body: Body::Encoded,
            error,
            wire_size: Self::sum_sizes(&fields),
            error_message: error.then(|| Self::error_message_field(&fields)).flatten(),
            fields,
            default_constructors,
            initializers,
            static_methods,
            instance_methods,
            doc: record.meta().doc().map(Javadoc::new),
        })
    }

    fn requires_wire_runtime(&self) -> bool {
        self.codec_payload()
            || self
                .initializers
                .iter()
                .chain(&self.static_methods)
                .chain(&self.instance_methods)
                .any(Call::requires_wire_runtime)
    }

    fn sum_sizes(fields: &[Field]) -> Expression {
        fields
            .iter()
            .map(|field| field.wire_size.clone())
            .reduce(Expression::add)
            .unwrap_or_else(|| Expression::integer(0))
    }

    fn error_message_field(fields: &[Field]) -> Option<Identifier> {
        let message = Identifier::known("message");
        let string = TypeName::named(TypeIdentifier::known("String", JavaVersion::JAVA_8));
        fields
            .iter()
            .find(|field| field.name == message && field.ty == string)
            .map(|field| field.name.clone())
    }
}

impl Field {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn direct_read(&self) -> Option<Expression> {
        match &self.storage {
            FieldStorage::Direct {
                primitive, offset, ..
            } => Some(
                (*primitive).buffer_read_at(
                    Expression::identifier(Identifier::known("buffer")),
                    Expression::identifier(Identifier::known("offset"))
                        .add(Expression::integer(*offset)),
                ),
            ),
            FieldStorage::Encoded => None,
        }
    }

    pub fn direct_write(&self) -> Option<Expression> {
        match &self.storage {
            FieldStorage::Direct {
                primitive, offset, ..
            } => Some(
                (*primitive).buffer_write_at(
                    Expression::identifier(Identifier::known("buffer")),
                    Expression::identifier(Identifier::known("offset"))
                        .add(Expression::integer(*offset)),
                    Expression::this().member(self.name.clone()),
                ),
            ),
            FieldStorage::Encoded => None,
        }
    }

    pub fn wire_read(&self) -> &Expression {
        &self.wire_read
    }

    pub fn wire_write(&self) -> &[Statement] {
        &self.wire_write
    }

    pub fn wire_size(&self) -> &Expression {
        &self.wire_size
    }

    pub fn equals(&self) -> &Expression {
        &self.equals
    }

    pub fn hash(&self) -> &Expression {
        &self.hash
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    pub fn native_record_safe(&self) -> bool {
        self.native_record_safe
    }

    pub fn requires_identity(&self) -> bool {
        self.requires_identity
    }

    fn from_direct(
        field: &DirectFieldDecl,
        record: &DirectRecordDecl<Native>,
        version: JavaVersion,
    ) -> Result<Self> {
        let name = ValueMemberAccess::RecordField.field(field.key(), version)?;
        let offset = record
            .layout()
            .field(field.key())
            .ok_or(JavaHost::broken_bridge_contract(
                "direct record field layout was not found",
            ))?
            .offset()
            .get();
        let primitive = Primitive::try_from(field.ty().primitive())?;
        let reader = Expression::identifier(Identifier::known("reader"));
        let writer = Expression::identifier(Identifier::known("writer"));
        let current = Expression::this().member(name.clone());
        let left = Expression::this().member(name.clone());
        let right = Expression::identifier(Identifier::known("other")).member(name.clone());
        let default = field
            .meta()
            .default()
            .map(|value| {
                DefaultExpression::render(
                    &boltffi_binding::TypeRef::Primitive(field.ty().primitive()),
                    value,
                    version,
                )
            })
            .transpose()?;
        Ok(Self {
            name,
            ty: TypeName::primitive(primitive),
            storage: FieldStorage::Direct { primitive, offset },
            wire_read: reader.call(
                Identifier::parse_for(format!("read{}", primitive.wire_method_suffix()), version)?,
                ArgumentList::default(),
            ),
            wire_write: vec![Statement::expression(writer.call(
                Identifier::parse_for(format!("write{}", primitive.wire_method_suffix()), version)?,
                [current].into_iter().collect(),
            ))],
            wire_size: Expression::integer(primitive.wire_size()),
            equals: primitive.equals(left.clone(), right),
            hash: primitive.hash(left),
            default,
            doc: field.meta().doc().map(Javadoc::new),
            native_record_safe: true,
            requires_identity: false,
        })
    }

    pub fn from_encoded(
        field: &EncodedFieldDecl,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_encoded_with_package(
            field,
            version,
            context,
            None,
            ValueMemberAccess::RecordField,
        )
    }

    pub fn from_enum_payload<'context>(
        field: &EncodedFieldDecl,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
        package: &'context JavaPackage,
    ) -> Result<Self> {
        Self::from_encoded_with_package(
            field,
            version,
            context,
            Some(package),
            ValueMemberAccess::RecordField,
        )
    }

    pub fn from_enum_tuple_payload<'context>(
        field: &EncodedFieldDecl,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
        package: &'context JavaPackage,
    ) -> Result<Self> {
        Self::from_encoded_with_package(
            field,
            version,
            context,
            Some(package),
            ValueMemberAccess::VariantField,
        )
    }

    fn from_encoded_with_package<'context>(
        field: &EncodedFieldDecl,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
        package: Option<&'context JavaPackage>,
        member_access: ValueMemberAccess,
    ) -> Result<Self> {
        let name = member_access.field(field.key(), version)?;
        let ty = match package {
            Some(package) => JavaType::qualified_field(field.ty(), version, context, package)?,
            None => JavaType::field(field.ty(), version, context)?,
        };
        let reader = Identifier::known("reader");
        let writer = Identifier::known("writer");
        let current = Expression::this();
        let mut codec_reader = Reader::new(reader, version, context);
        if let Some(package) = package {
            codec_reader = codec_reader.package(package);
        }
        let wire_read = field
            .read()
            .render_with(&mut codec_reader)?
            .into_expression();
        let mut writer = Writer::new(writer, version, context)
            .current(current.clone())
            .members(member_access);
        let wire_write = field
            .write()
            .render_with(&mut writer)
            .into_iter()
            .map(|statement| {
                statement.map(crate::target::java::codec::WriteStatement::into_statement)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut sizer = Sizer::new(version, context)
            .current(current)
            .members(member_access);
        let wire_size = field.write().size_with(&mut sizer)?.into_expression();
        let left = Expression::this().member(name.clone());
        let right = Expression::identifier(Identifier::known("other")).member(name.clone());
        let default = field
            .meta()
            .default()
            .map(|value| DefaultExpression::render(field.ty(), value, version))
            .transpose()?;
        Ok(Self {
            name,
            ty: ty.ty().clone(),
            storage: FieldStorage::Encoded,
            wire_read,
            wire_write,
            wire_size,
            equals: ty.equals(left.clone(), right, version)?,
            hash: ty.hash(left, version)?,
            default,
            doc: field.meta().doc().map(Javadoc::new),
            native_record_safe: ty.native_record_safe(),
            requires_identity: ty.requires_identity(),
        })
    }
}

impl DefaultConstructor {
    pub fn parameters(&self) -> &[Parameter<TypeName>] {
        &self.parameters
    }

    pub fn arguments(&self) -> &ArgumentList {
        &self.arguments
    }

    fn from_fields(fields: &[Field]) -> Vec<Self> {
        let trailing_defaults = fields
            .iter()
            .rev()
            .take_while(|field| field.default.is_some())
            .count();
        (1..=trailing_defaults)
            .map(|omitted| {
                let included = fields.len() - omitted;
                Self {
                    parameters: fields
                        .iter()
                        .take(included)
                        .map(|field| Parameter::new(field.name.clone(), field.ty.clone()))
                        .collect(),
                    arguments: fields
                        .iter()
                        .enumerate()
                        .map(|(index, field)| match index < included {
                            true => Expression::identifier(field.name.clone()),
                            false => field
                                .default
                                .clone()
                                .expect("omitted record fields have defaults"),
                        })
                        .collect(),
                }
            })
            .collect()
    }
}
