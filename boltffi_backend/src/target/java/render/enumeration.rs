use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, CStyleVariantDecl, ConstantOwner, DataEnumDecl, DataVariantDecl,
    DataVariantPayload, EnumDecl, EnumId, Native,
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
            AssociatedConstants, Constant, ValueIdentity,
            call::{AssociatedCallContext, Call, ValueCalls, ValueReceiver},
            record::Field,
            type_name::JavaType,
        },
        syntax::{
            ArgumentList, Expression, Identifier, Javadoc, Statement, TypeIdentifier, TypeName,
        },
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
    constants: AssociatedConstants,
    calls: ValueCalls,
    doc: Option<Javadoc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Data {
    name: TypeIdentifier,
    form: DataForm,
    error: bool,
    variants: Vec<DataVariant>,
    constants: AssociatedConstants,
    calls: ValueCalls,
    doc: Option<Javadoc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataForm {
    Sealed,
    Abstract,
    FlatError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VariantInitialization {
    External,
    OwningType,
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

    pub fn unit_variant_expression(
        id: EnumId,
        variant_name: &boltffi_binding::CanonicalName,
        initialization: VariantInitialization,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let enumeration = context
            .enumeration(id)
            .ok_or(JavaHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))?;
        let owner = TypeName::named(Name::new(enumeration.name()).type_name(version)?);
        match enumeration {
            EnumDecl::CStyle(_) => Ok(Expression::static_member(
                owner,
                Name::new(variant_name).enum_entry(version)?,
            )),
            EnumDecl::Data(enumeration) => DataForm::classify(enumeration, version, context)?
                .unit_variant_expression(enumeration, owner, variant_name, initialization, version),
            _ => Err(JavaHost::unsupported("unknown enum variant value")),
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

    pub fn constants(&self) -> &[Constant] {
        self.constants.as_slice()
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
            constants: AssociatedConstants::from_c_style_enum(
                enumeration,
                bridge,
                native_owner,
                version,
                context,
            )?,
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
        let emitted = self
            .calls
            .iter()
            .try_fold(emitted, |emitted, call| -> Result<_> {
                Ok(call
                    .native_forwards()?
                    .into_iter()
                    .fold(emitted, Emitted::with_aux))
            })?;
        self.constants.extend(emitted)
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

    pub fn constants(&self) -> &[Constant] {
        self.constants.as_slice()
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
        if enumeration.has_transparent_variants() {
            return Err(JavaHost::unsupported("transparent data enum variant"));
        }
        let name = Name::new(enumeration.name()).type_name(version)?;
        let variants = enumeration
            .variants()
            .iter()
            .map(|variant| DataVariant::from_declaration(variant, package, version, context))
            .collect::<Result<Vec<_>>>()?;
        let error = enumeration.is_error_payload();
        let form = DataForm::classify(enumeration, version, context)?;
        Ok(Self {
            name: name.clone(),
            form,
            error,
            variants,
            constants: AssociatedConstants::from_owner(
                ConstantOwner::Enum(enumeration.id()),
                bridge,
                native_owner,
                version,
                context,
            )?,
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
        let emitted = self
            .calls
            .iter()
            .try_fold(emitted, |emitted, call| -> Result<_> {
                Ok(call
                    .native_forwards()?
                    .into_iter()
                    .fold(emitted, Emitted::with_aux))
            })?;
        self.constants.extend(emitted)
    }
}

impl DataForm {
    fn classify(
        enumeration: &DataEnumDecl<Native>,
        version: JavaVersion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if enumeration.is_error_payload()
            && enumeration
                .variants()
                .iter()
                .all(|variant| variant.payload().is_unit())
        {
            return Ok(Self::FlatError);
        }
        let native_record_safe = enumeration
            .variants()
            .iter()
            .flat_map(|variant| variant.payload().fields())
            .try_fold(true, |safe, field| match safe {
                true => JavaType::field(field.ty(), version, context)
                    .map(|field| field.native_record_safe()),
                false => Ok(false),
            })?;
        match !enumeration.is_error_payload() && version.supports_sealed() && native_record_safe {
            true => Ok(Self::Sealed),
            false => Ok(Self::Abstract),
        }
    }

    fn unit_variant_expression(
        self,
        enumeration: &DataEnumDecl<Native>,
        owner: TypeName,
        variant_name: &boltffi_binding::CanonicalName,
        initialization: VariantInitialization,
        version: JavaVersion,
    ) -> Result<Expression> {
        let variant = enumeration
            .variants()
            .iter()
            .find(|variant| variant.name() == variant_name && variant.payload().is_unit())
            .ok_or_else(|| JavaHost::unsupported("data enum unit variant"))?;
        let variant = Name::new(variant.name()).variant(version)?;
        match (self, initialization) {
            (Self::FlatError, _) => Ok(Expression::static_member(
                owner,
                Identifier::parse_for(variant.as_str(), version)?,
            )),
            (Self::Sealed, _) | (Self::Abstract, VariantInitialization::OwningType) => Ok(
                Expression::construct(TypeName::nested(owner, variant), ArgumentList::default()),
            ),
            (Self::Abstract, VariantInitialization::External) => Ok(Expression::static_member(
                TypeName::nested(owner, variant),
                Identifier::known("INSTANCE"),
            )),
        }
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
