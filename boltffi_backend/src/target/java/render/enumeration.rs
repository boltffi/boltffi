use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, CStyleVariantDecl, DataEnumDecl, DataVariantDecl, DataVariantPayload, EnumDecl,
    EnumId, Native,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::java::{
        JavaFile, JavaHost, JavaPackage, JavaVersion,
        admission::EnumShape,
        codec::Runtime,
        name_style::Name,
        primitive::Primitive,
        render::{
            ValueIdentity,
            call::{AssociatedCallContext, Call, ValueCalls, ValueReceiver},
            record::Field,
        },
        syntax::{Expression, Identifier, Javadoc, Statement, TypeIdentifier, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/enumeration/c_style.java", escape = "none")]
struct CStyleTemplate<'enumeration> {
    package: &'enumeration JavaPackage,
    enumeration: &'enumeration CStyle,
}

#[derive(AskamaTemplate)]
#[template(path = "target/java/enumeration/data.java", escape = "none")]
struct DataTemplate<'enumeration> {
    package: &'enumeration JavaPackage,
    enumeration: &'enumeration Data,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Enumeration {
    CStyle(CStyle),
    Data(Data),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CStyle {
    name: TypeIdentifier,
    value_type: TypeName,
    long_value: bool,
    error: bool,
    variants: Vec<CStyleVariant>,
    calls: ValueCalls,
    doc: Option<Javadoc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Data {
    name: TypeIdentifier,
    form: DataForm,
    error: bool,
    variants: Vec<DataVariant>,
    calls: ValueCalls,
    doc: Option<Javadoc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataForm {
    Sealed,
    Abstract,
    FlatError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CStyleVariant {
    name: Identifier,
    value: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataVariant {
    name: TypeIdentifier,
    tag: Expression,
    fields: Vec<Field>,
    size: Expression,
    doc: Option<Javadoc>,
}

impl Enumeration {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        package: &JavaPackage,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        EnumShape::classify(declaration).require_supported()?;
        match declaration {
            EnumDecl::CStyle(enumeration) => CStyle::from_declaration(
                enumeration,
                bridge,
                native_owner,
                package,
                version,
                context,
            )
            .map(Self::CStyle),
            EnumDecl::Data(enumeration) => {
                Data::from_declaration(enumeration, bridge, native_owner, package, version, context)
                    .map(Self::Data)
            }
            _ => Err(JavaHost::unsupported("unknown enum declaration")),
        }
    }

    pub fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        match self {
            Self::CStyle(enumeration) => enumeration.render(package),
            Self::Data(enumeration) => enumeration.render(package),
        }
    }

    pub fn calls(&self) -> &ValueCalls {
        match self {
            Self::CStyle(enumeration) => enumeration.calls(),
            Self::Data(enumeration) => enumeration.calls(),
        }
    }

    pub fn file_for(declaration: &EnumDecl<Native>, version: JavaVersion) -> Result<JavaFile> {
        Name::new(declaration.name())
            .type_name(version)
            .and_then(|name| JavaFile::parse_for(name.as_str(), version))
    }

    pub fn type_name_for(
        id: EnumId,
        context: &RenderContext<Native>,
        version: JavaVersion,
    ) -> Result<TypeIdentifier> {
        context
            .enumeration(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))
            .and_then(|enumeration| Name::new(enumeration.name()).type_name(version))
    }

    pub fn c_style_primitive(id: EnumId, context: &RenderContext<Native>) -> Result<Primitive> {
        match context
            .enumeration(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))? {
            EnumDecl::CStyle(enumeration) => Primitive::try_from(enumeration.repr().primitive()),
            EnumDecl::Data(_) => Err(JavaHost::unsupported("data enum direct carrier")),
            _ => Err(JavaHost::unsupported("unknown enum direct carrier")),
        }
    }
}

impl CStyle {
    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub fn value_type(&self) -> &TypeName {
        &self.value_type
    }

    pub fn long_value(&self) -> bool {
        self.long_value
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn variants(&self) -> &[CStyleVariant] {
        &self.variants
    }

    pub fn calls(&self) -> &ValueCalls {
        &self.calls
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    fn from_declaration(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        package: &JavaPackage,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(enumeration.name()).type_name(version)?;
        let source_primitive = enumeration.repr().primitive();
        let primitive = Primitive::try_from(source_primitive)?;
        Ok(Self {
            name: name.clone(),
            value_type: TypeName::primitive(primitive),
            long_value: primitive == Primitive::Long,
            error: enumeration.is_error_payload(),
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| CStyleVariant::from_declaration(variant, source_primitive, version))
                .collect::<Result<Vec<_>>>()?,
            calls: ValueCalls::from_declarations(
                enumeration.initializers(),
                enumeration.methods(),
                ValueReceiver::DirectEnum(name),
                AssociatedCallContext::nested(bridge, native_owner, package, version, context),
            )?,
            doc: enumeration.meta().doc().map(Javadoc::new),
        })
    }

    fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        let emitted = Emitted::primary(
            CStyleTemplate {
                package,
                enumeration: self,
            }
            .render()?,
        );
        let emitted = match self.calls.iter().any(Call::requires_async_runtime) {
            true => emitted.with_aux(Runtime::async_helper()?),
            false => emitted,
        };
        self.calls.iter().try_fold(emitted, |emitted, call| {
            Ok(call
                .native_forwards()?
                .into_iter()
                .fold(emitted, Emitted::with_aux))
        })
    }
}

impl Data {
    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub fn sealed(&self) -> bool {
        self.form == DataForm::Sealed
    }

    pub fn flat_error(&self) -> bool {
        self.form == DataForm::FlatError
    }

    pub fn variants(&self) -> &[DataVariant] {
        &self.variants
    }

    pub fn calls(&self) -> &ValueCalls {
        &self.calls
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    pub fn error(&self) -> bool {
        self.error
    }

    fn from_declaration(
        enumeration: &DataEnumDecl<Native>,
        bridge: &JniBridgeContract,
        native_owner: &TypeIdentifier,
        package: &JavaPackage,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(enumeration.name()).type_name(version)?;
        let variants = enumeration
            .variants()
            .iter()
            .map(|variant| DataVariant::from_declaration(variant, package, version, context))
            .collect::<Result<Vec<_>>>()?;
        let error = enumeration.is_error_payload();
        let form = if error && variants.iter().all(DataVariant::unit) {
            DataForm::FlatError
        } else if !error
            && version.supports_sealed()
            && variants
                .iter()
                .flat_map(|variant| variant.fields.iter())
                .all(Field::native_record_safe)
        {
            DataForm::Sealed
        } else {
            DataForm::Abstract
        };
        Ok(Self {
            name: name.clone(),
            form,
            error,
            variants,
            calls: ValueCalls::from_declarations(
                enumeration.initializers(),
                enumeration.methods(),
                ValueReceiver::Encoded {
                    ty: name,
                    codec: enumeration.write().clone(),
                },
                AssociatedCallContext::nested(bridge, native_owner, package, version, context),
            )?,
            doc: enumeration.meta().doc().map(Javadoc::new),
        })
    }

    fn render(&self, package: &JavaPackage) -> Result<Emitted> {
        let emitted = Emitted::primary(
            DataTemplate {
                package,
                enumeration: self,
            }
            .render()?,
        )
        .with_aux(Runtime::helper()?);
        let emitted = match self
            .calls()
            .iter()
            .any(|call| call.requires_direct_vector_runtime())
        {
            true => emitted.with_aux(Runtime::direct_vector_helper()?),
            false => emitted,
        };
        let emitted = match self
            .calls()
            .iter()
            .any(|call| call.requires_async_runtime())
        {
            true => emitted.with_aux(Runtime::async_helper()?),
            false => emitted,
        };
        let emitted = match self
            .variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .any(Field::requires_identity)
        {
            true => emitted.with_aux(ValueIdentity::helper()?),
            false => emitted,
        };
        self.calls.iter().try_fold(emitted, |emitted, call| {
            Ok(call
                .native_forwards()?
                .into_iter()
                .fold(emitted, Emitted::with_aux))
        })
    }
}

impl CStyleVariant {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }

    fn from_declaration(
        variant: &CStyleVariantDecl,
        source_primitive: boltffi_binding::Primitive,
        version: JavaVersion,
    ) -> Result<Self> {
        let primitive = Primitive::try_from(source_primitive)?;
        Ok(Self {
            name: Name::new(variant.name()).enum_entry(version)?,
            value: primitive.integer_literal(source_primitive, variant.discriminant())?,
        })
    }
}

impl DataVariant {
    pub fn name(&self) -> &TypeIdentifier {
        &self.name
    }

    pub fn tag(&self) -> &Expression {
        &self.tag
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn size(&self) -> &Expression {
        &self.size
    }

    pub fn doc(&self) -> Option<&Javadoc> {
        self.doc.as_ref()
    }

    pub fn unit(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn message_field(&self) -> Option<&Identifier> {
        let message = Identifier::known("message");
        let string = TypeName::named(TypeIdentifier::known("String", JavaVersion::JAVA_8));
        self.fields
            .iter()
            .find(|field| field.name() == &message && field.ty() == &string)
            .map(Field::name)
    }

    fn from_declaration(
        variant: &DataVariantDecl,
        package: &JavaPackage,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let fields = match variant.payload() {
            DataVariantPayload::Unit => Vec::new(),
            DataVariantPayload::Tuple(fields) => fields
                .iter()
                .map(|field| Field::from_enum_tuple_payload(field, version, context, package))
                .collect::<Result<Vec<_>>>()?,
            DataVariantPayload::Struct(fields) => fields
                .iter()
                .map(|field| Field::from_enum_payload(field, version, context, package))
                .collect::<Result<Vec<_>>>()?,
            _ => return Err(JavaHost::unsupported("unknown data enum payload")),
        };
        let size = fields
            .iter()
            .map(Field::wire_size)
            .cloned()
            .fold(Expression::integer(4), Expression::add);
        Ok(Self {
            name: Name::new(variant.name()).variant(version)?,
            tag: Expression::integer(u64::from(variant.tag().get())),
            fields,
            size,
            doc: variant.meta().doc().map(Javadoc::new),
        })
    }

    pub fn tag_write(&self) -> Statement {
        Statement::expression(Expression::identifier(Identifier::known("writer")).call(
            Identifier::known("writeInt"),
            [self.tag.clone()].into_iter().collect(),
        ))
    }
}
