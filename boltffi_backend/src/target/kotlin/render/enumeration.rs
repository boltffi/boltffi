use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, CStyleVariantDecl, ConstantOwner, DataEnumDecl, DataVariantDecl,
    DataVariantPayload, EnumDecl, EnumId, ExportedMethodDecl, InitializerDecl, Native,
    NativeSymbol, Primitive, Receive, VariantTag,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::WireBuffer,
        name_style::KotlinPackage,
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            AssociatedConstants, Documentation,
            field::EncodedField,
            function::{ExportedCall, ExportedCallRenderer, ReceiverCarrier, ReceiverMutation},
        },
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/enumeration.kt", escape = "none")]
struct EnumerationTemplate {
    enumeration: Enumeration,
    constants: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enumeration {
    name: TypeName,
    documentation: Documentation,
    error: bool,
    body: Body,
    constants: AssociatedConstants,
    initializers: Vec<ExportedCall>,
    static_methods: Vec<ExportedCall>,
    instance_methods: Vec<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    CStyle {
        value_type: TypeName,
        repr: Primitive,
        variants: Vec<CStyleVariant>,
    },
    Data {
        variants: Vec<DataVariant>,
        wire_size_type: TypeName,
        /// Whether any variant renders as its payload record. A transparent
        /// enum renders as a sealed interface whose codec lives on the
        /// companion, because the variant tag depends on which enum a shared
        /// payload record is written through and so cannot be a member of
        /// the payload.
        transparent: bool,
    },
}

const KOTLIN_SHADOWABLE_PRIMITIVES: &[&str] = &[
    "Boolean", "Byte", "UByte", "Short", "UShort", "Int", "UInt", "Long", "ULong", "Float",
    "Double",
];

fn shadowed_primitive_names(enum_name: &TypeName, variant_names: &[String]) -> Vec<String> {
    let enum_name = enum_name.to_string();
    KOTLIN_SHADOWABLE_PRIMITIVES
        .iter()
        .filter(|primitive| {
            enum_name == **primitive || variant_names.iter().any(|variant| variant == *primitive)
        })
        .map(|primitive| (*primitive).to_string())
        .collect()
}

fn qualify_shadowed(ty: TypeName, shadowed: &[String]) -> TypeName {
    if shadowed.is_empty() {
        return ty;
    }
    let spelling = ty.to_string();
    let mut result = String::with_capacity(spelling.len());
    let mut token = String::new();
    let mut preceding = None;
    for ch in spelling.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            token.push(ch);
        } else {
            flush_token(&mut result, &mut token, preceding, shadowed);
            result.push(ch);
            preceding = Some(ch);
        }
    }
    flush_token(&mut result, &mut token, preceding, shadowed);
    if result == spelling {
        ty
    } else {
        TypeName::new(result)
    }
}

fn flush_token(
    result: &mut String,
    token: &mut String,
    preceding: Option<char>,
    shadowed: &[String],
) {
    if token.is_empty() {
        return;
    }
    if preceding != Some('.') && shadowed.iter().any(|name| name == token) {
        result.push_str("kotlin.");
    }
    result.push_str(token);
    token.clear();
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CStyleVariant {
    name: Identifier,
    documentation: Documentation,
    value: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataVariant {
    name: Identifier,
    documentation: Documentation,
    tag: Expression,
    fields: Vec<EncodedField>,
    read: Expression,
    size: Expression,
    tag_write: Statement,
    transparent: bool,
    /// The type the companion codec dispatch matches with `is`: the payload
    /// record for a transparent variant, the nested variant class otherwise.
    subject: TypeName,
    /// The payload write statements of the companion codec dispatch, after
    /// the tag write.
    payload_writes: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Receiver {
    carrier: ReceiverCarrier,
    writeback: Option<TypeName>,
}

impl Enumeration {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, None, host, context),
            EnumDecl::Data(enumeration) => Self::from_data(enumeration, None, host, context, None),
            _ => Err(KotlinHost::unsupported("unknown enum declaration")),
        }
    }

    pub fn from_declaration_with_package(
        declaration: &EnumDecl<Native>,
        host: &KotlinHost,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => {
                Self::from_c_style(enumeration, Some(bridge), host, context)
            }
            EnumDecl::Data(enumeration) => {
                Self::from_data(enumeration, Some(bridge), host, context, package)
            }
            _ => Err(KotlinHost::unsupported("unknown enum declaration")),
        }
    }

    pub fn from_id(id: EnumId, host: &KotlinHost, context: &RenderContext<Native>) -> Result<Self> {
        context
            .enumeration(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))
            .and_then(|enumeration| Self::from_declaration(enumeration, host, context))
    }

    pub fn render(self) -> Result<Emitted> {
        let constants = self.constants.render("        ")?;
        Ok(Emitted::primary(
            EnumerationTemplate {
                enumeration: self,
                constants,
            }
            .render()?,
        ))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    pub fn c_style(&self) -> bool {
        matches!(&self.body, Body::CStyle { .. })
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn data(&self) -> bool {
        matches!(&self.body, Body::Data { .. })
    }

    pub fn transparent(&self) -> bool {
        matches!(
            &self.body,
            Body::Data {
                transparent: true,
                ..
            }
        )
    }

    pub fn value_type(&self) -> Option<&TypeName> {
        match &self.body {
            Body::CStyle { value_type, .. } => Some(value_type),
            Body::Data { .. } => None,
        }
    }

    pub fn repr(&self) -> Result<Primitive> {
        match &self.body {
            Body::CStyle { repr, .. } => Ok(*repr),
            Body::Data { .. } => Err(KotlinHost::unsupported("data enum has no direct repr")),
        }
    }

    pub fn c_style_variants(&self) -> &[CStyleVariant] {
        match &self.body {
            Body::CStyle { variants, .. } => variants,
            Body::Data { .. } => &[],
        }
    }

    pub fn data_variants(&self) -> &[DataVariant] {
        match &self.body {
            Body::Data { variants, .. } => variants,
            Body::CStyle { .. } => &[],
        }
    }

    pub fn wire_size_type(&self) -> TypeName {
        match &self.body {
            Body::Data { wire_size_type, .. } => wire_size_type.clone(),
            Body::CStyle { .. } => TypeName::int(),
        }
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

    pub fn unknown_tag(&self) -> Expression {
        Expression::throw_illegal_argument(Literal::interpolated_string(&format!(
            "unknown {self} tag: $tag"
        )))
    }

    pub fn type_name_from_id(id: EnumId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .enumeration(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))
            .map(|enumeration| Name::new(enumeration.name()).type_name())
    }

    pub fn native_argument(
        id: EnumId,
        value: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let primitive = match context
            .enumeration(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))? {
            EnumDecl::CStyle(enumeration) => enumeration.repr().primitive(),
            EnumDecl::Data(_) => {
                return Err(KotlinHost::unsupported("data enum direct argument"));
            }
            _ => {
                return Err(KotlinHost::unsupported("unknown enum declaration"));
            }
        };
        KotlinPrimitive::new(primitive)
            .native_argument(Expression::property(value, Identifier::parse("value")?))
    }

    pub fn write_statement(
        id: EnumId,
        value: Expression,
        writer: Identifier,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Statement> {
        // A transparent enum keeps its codec on the companion: the payload
        // record cannot carry a `writeTo` member because the tag it must
        // write depends on which enum it is written through.
        if Self::is_transparent(id, context)? {
            let ty = Self::qualified_name_from_id(id, context, package)?;
            return Ok(Statement::expression(Expression::call(
                ty,
                Identifier::parse("writeTo")?,
                [value, Expression::identifier(writer)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )));
        }
        Self::type_name_from_id(id, context).and_then(|_| {
            Ok(Statement::expression(Expression::call(
                value,
                Identifier::parse("writeTo")?,
                [Expression::identifier(writer)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )))
        })
    }

    pub fn size_expression(
        id: EnumId,
        value: Expression,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Expression> {
        if Self::is_transparent(id, context)? {
            let ty = Self::qualified_name_from_id(id, context, package)?;
            return Ok(Expression::call(
                ty,
                Identifier::parse("wireSize")?,
                [value].into_iter().collect::<ArgumentList>(),
            ));
        }
        Self::type_name_from_id(id, context).and_then(|_| {
            Ok(Expression::call(
                value,
                Identifier::parse("wireSize")?,
                ArgumentList::default(),
            ))
        })
    }

    fn is_transparent(id: EnumId, context: &RenderContext<Native>) -> Result<bool> {
        match context
            .enumeration(id)
            .ok_or(KotlinHost::broken_bridge_contract(
                "enum type was not found in render context",
            ))? {
            EnumDecl::Data(enumeration) => Ok(enumeration.has_transparent_variants()),
            _ => Ok(false),
        }
    }

    fn qualified_name_from_id(
        id: EnumId,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<TypeName> {
        let ty = Self::type_name_from_id(id, context)?;
        Ok(match package {
            Some(package) => TypeName::qualified(package, ty),
            None => ty,
        })
    }

    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: Option<&JniBridgeContract>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let error = enumeration.is_error_payload();
        let primitive = enumeration.repr().primitive();
        let name = Name::new(enumeration.name()).type_name();
        let receiver = Receiver {
            carrier: ReceiverCarrier::direct(KotlinPrimitive::new(primitive).native_argument(
                Expression::property(Expression::this(), Identifier::parse("value")?),
            )?),
            writeback: None,
        };
        Ok(Self {
            name,
            documentation: Documentation::new(enumeration.meta().doc()),
            error,
            constants: AssociatedConstants::from_c_style_enum(enumeration, host, bridge, context)?,
            body: Body::CStyle {
                value_type: KotlinPrimitive::new(primitive).native_type()?,
                repr: primitive,
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| CStyleVariant::from_c_style(variant, enumeration, error))
                    .collect::<Result<Vec<_>>>()?,
            },
            initializers: Self::initializer_calls(
                enumeration.initializers(),
                bridge,
                host,
                context,
                None,
            )?,
            static_methods: Self::methods(
                enumeration.methods(),
                None,
                bridge,
                host,
                context,
                None,
            )?,
            instance_methods: Self::methods(
                enumeration.methods(),
                Some(receiver),
                bridge,
                host,
                context,
                None,
            )?,
        })
    }

    fn from_data(
        enumeration: &DataEnumDecl<Native>,
        bridge: Option<&JniBridgeContract>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        let error = enumeration.is_error_payload();
        let transparent = enumeration.has_transparent_variants();
        if error && transparent {
            // The sealed interface a transparent enum renders as cannot
            // extend Exception the way the error sealed class does.
            return Err(KotlinHost::unsupported("transparent error enum"));
        }
        let name = Name::new(enumeration.name()).type_name();
        let buffer = WireBuffer::new(&Name::new(enumeration.name()))?;
        let writer = buffer.writer().clone();
        // The codec of a transparent enum lives on the companion, so the
        // receiver encodes through `X.wireSize(this)` / `X.writeTo(this, w)`
        // instead of member calls.
        let receiver = Receiver {
            carrier: ReceiverCarrier::encoded(match transparent {
                true => buffer.write_statements(
                    Expression::call(
                        name.clone(),
                        Identifier::parse("wireSize")?,
                        [Expression::this()].into_iter().collect::<ArgumentList>(),
                    ),
                    vec![Statement::expression(Expression::call(
                        name.clone(),
                        Identifier::parse("writeTo")?,
                        [Expression::this(), Expression::identifier(writer)]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ))],
                )?,
                false => buffer.write_statements(
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
                )?,
            }),
            writeback: Some(name.clone()),
        };
        let variant_names = enumeration
            .variants()
            .iter()
            .map(|variant| {
                Name::new(variant.name())
                    .variant()
                    .map(|name| name.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let shadowed = shadowed_primitive_names(&name, &variant_names);
        let wire_size_type = qualify_shadowed(TypeName::int(), &shadowed);
        let requalify = |calls: Vec<ExportedCall>| {
            calls
                .into_iter()
                .map(|call| call.requalify_types(&|ty| qualify_shadowed(ty, &shadowed)))
                .collect::<Vec<_>>()
        };
        // In the classic sealed class the codec methods are members, so the
        // field expressions hang off `this`; the transparent companion
        // dispatches over a `value` parameter instead.
        let current = match transparent {
            true => Expression::identifier(Identifier::parse("value")?),
            false => Expression::this(),
        };
        Ok(Self {
            documentation: Documentation::new(enumeration.meta().doc()),
            body: Body::Data {
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| {
                        DataVariant::from_declaration(
                            variant,
                            host,
                            context,
                            package,
                            &shadowed,
                            current.clone(),
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
                wire_size_type,
                transparent,
            },
            constants: AssociatedConstants::from_owner(
                ConstantOwner::Enum(enumeration.id()),
                host,
                bridge,
                context,
            )?,
            initializers: requalify(Self::initializer_calls(
                enumeration.initializers(),
                bridge,
                host,
                context,
                package,
            )?),
            static_methods: requalify(Self::methods(
                enumeration.methods(),
                None,
                bridge,
                host,
                context,
                package,
            )?),
            instance_methods: requalify(Self::methods(
                enumeration.methods(),
                Some(receiver),
                bridge,
                host,
                context,
                package,
            )?),
            name,
            error,
        })
    }

    fn initializer_calls(
        initializers: &[InitializerDecl<Native>],
        bridge: Option<&JniBridgeContract>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Vec<ExportedCall>> {
        bridge.map_or_else(
            || Ok(Vec::new()),
            |bridge| {
                let calls = ExportedCallRenderer::new(host, bridge, context);
                initializers
                    .iter()
                    .map(|initializer| {
                        match package {
                            Some(package) => calls.with_package(
                                Name::new(initializer.name()).function()?,
                                initializer.symbol(),
                                initializer.callable(),
                                None,
                                package,
                            ),
                            None => calls.exported(
                                Name::new(initializer.name()).function()?,
                                initializer.symbol(),
                                initializer.callable(),
                                None,
                            ),
                        }
                        .map(|call| call.documented(initializer.meta().doc()))
                    })
                    .collect()
            },
        )
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        bridge: Option<&JniBridgeContract>,
        host: &KotlinHost,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Vec<ExportedCall>> {
        bridge.map_or_else(
            || Ok(Vec::new()),
            |bridge| {
                let calls = ExportedCallRenderer::new(host, bridge, context);
                methods
                    .iter()
                    .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
                    .map(|method| {
                        match (method.callable().receiver(), receiver.clone()) {
                            (Some(Receive::ByMutRef), Some(receiver)) => receiver
                                .writeback
                                .ok_or(KotlinHost::unsupported("mutable c-style enum receiver"))
                                .and_then(|writeback| {
                                    let mutation = match package {
                                        Some(package) => ReceiverMutation::encoded(writeback)
                                            .with_package(package),
                                        None => ReceiverMutation::encoded(writeback),
                                    };
                                    calls.with_receiver_mutation(
                                        Name::new(method.name()).function()?,
                                        method.target(),
                                        method.callable(),
                                        receiver.carrier,
                                        mutation,
                                    )
                                }),
                            (Some(Receive::ByRef | Receive::ByValue), Some(receiver)) => {
                                match package {
                                    Some(package) => calls.with_package(
                                        Name::new(method.name()).function()?,
                                        method.target(),
                                        method.callable(),
                                        Some(receiver.carrier),
                                        package,
                                    ),
                                    None => calls.exported(
                                        Name::new(method.name()).function()?,
                                        method.target(),
                                        method.callable(),
                                        Some(receiver.carrier),
                                    ),
                                }
                            }
                            (None, None) => match package {
                                Some(package) => calls.with_package(
                                    Name::new(method.name()).function()?,
                                    method.target(),
                                    method.callable(),
                                    None,
                                    package,
                                ),
                                None => calls.exported(
                                    Name::new(method.name()).function()?,
                                    method.target(),
                                    method.callable(),
                                    None,
                                ),
                            },
                            _ => Err(KotlinHost::unsupported("enum method receiver")),
                        }
                        .map(|call| call.documented(method.meta().doc()))
                    })
                    .collect()
            },
        )
    }
}

impl std::fmt::Display for Enumeration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.name, formatter)
    }
}

impl CStyleVariant {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }

    fn from_c_style(
        variant: &CStyleVariantDecl,
        enumeration: &CStyleEnumDecl<Native>,
        error: bool,
    ) -> Result<Self> {
        Ok(Self {
            name: match error {
                true => Name::new(variant.name()).variant()?,
                false => Name::new(variant.name()).enum_entry()?,
            },
            documentation: Documentation::new(variant.meta().doc()),
            value: KotlinPrimitive::new(enumeration.repr().primitive())
                .native_integer_literal(variant.discriminant())?,
        })
    }
}

impl DataVariant {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    pub fn tag(&self) -> &Expression {
        &self.tag
    }

    pub fn fields(&self) -> &[EncodedField] {
        &self.fields
    }

    pub fn read(&self) -> &Expression {
        &self.read
    }

    pub fn size(&self) -> &Expression {
        &self.size
    }

    pub fn tag_write(&self) -> &Statement {
        &self.tag_write
    }

    pub fn unit(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn transparent(&self) -> bool {
        self.transparent
    }

    pub fn subject(&self) -> &TypeName {
        &self.subject
    }

    pub fn payload_writes(&self) -> &[Statement] {
        &self.payload_writes
    }

    fn from_declaration(
        variant: &DataVariantDecl,
        host: &KotlinHost,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
        shadowed: &[String],
        current: Expression,
    ) -> Result<Self> {
        let name = Name::new(variant.name()).variant()?;
        let tag = Self::tag_expression(variant.tag())?;
        let fields =
            Self::payload_fields(variant.payload(), host, context, package, shadowed, current)?;
        let tag_write = Statement::expression(Expression::call(
            Expression::identifier(Identifier::parse("writer")?),
            Identifier::parse("writeU32")?,
            [tag.clone()].into_iter().collect::<ArgumentList>(),
        ));
        if variant.transparent() {
            return Self::from_transparent(variant, name, tag, tag_write, fields);
        }
        let read = Self::read_expression(name.clone(), &fields);
        let size = fields
            .iter()
            .map(|field| field.size().clone())
            .fold(Expression::integer(4), Expression::add);
        let payload_writes = fields
            .iter()
            .map(|field| field.write().clone())
            .collect::<Vec<_>>();
        Ok(Self {
            subject: TypeName::new(name.to_string()),
            name,
            documentation: Documentation::new(variant.meta().doc()),
            tag,
            fields,
            read,
            size,
            tag_write,
            transparent: false,
            payload_writes,
        })
    }

    /// A transparent variant has no class of its own: dispatch matches the
    /// payload record type, the payload writes itself, and reads produce the
    /// payload directly. Lowering pinned the payload to exactly one record,
    /// so the record codec methods are the whole wire shape.
    fn from_transparent(
        variant: &DataVariantDecl,
        name: Identifier,
        tag: Expression,
        tag_write: Statement,
        fields: Vec<EncodedField>,
    ) -> Result<Self> {
        let [field] = fields.as_slice() else {
            return Err(KotlinHost::broken_bridge_contract(
                "transparent variant was not lowered with exactly one payload field",
            ));
        };
        let subject = field.ty().clone();
        let payload = Expression::identifier(Identifier::parse("value")?);
        Ok(Self {
            subject,
            documentation: Documentation::new(variant.meta().doc()),
            read: field.read().clone(),
            size: Expression::add(
                Expression::integer(4),
                Expression::call(
                    payload.clone(),
                    Identifier::parse("wireSize")?,
                    ArgumentList::default(),
                ),
            ),
            payload_writes: vec![Statement::expression(Expression::call(
                payload,
                Identifier::parse("writeTo")?,
                [Expression::identifier(Identifier::parse("writer")?)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))],
            name,
            tag,
            fields,
            tag_write,
            transparent: true,
        })
    }

    fn payload_fields(
        payload: &DataVariantPayload,
        host: &KotlinHost,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
        shadowed: &[String],
        current: Expression,
    ) -> Result<Vec<EncodedField>> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        match payload {
            DataVariantPayload::Unit => Ok(Vec::new()),
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => fields
                .iter()
                .map(|field| match package {
                    Some(package) => EncodedField::from_enum_payload(
                        field,
                        host,
                        context,
                        &reader,
                        &writer,
                        current.clone(),
                        package,
                    ),
                    None => EncodedField::from_declaration(
                        field,
                        host,
                        context,
                        &reader,
                        &writer,
                        current.clone(),
                    ),
                })
                .map(|field| {
                    field.map(|field| {
                        let qualified = qualify_shadowed(field.ty().clone(), shadowed);
                        field.requalified(qualified)
                    })
                })
                .collect(),
            _ => Err(KotlinHost::unsupported("unknown data enum payload")),
        }
    }

    fn read_expression(name: Identifier, fields: &[EncodedField]) -> Expression {
        match fields.is_empty() {
            true => Expression::identifier(name),
            false => Expression::construct(
                TypeName::new(name.to_string()),
                fields
                    .iter()
                    .map(|field| field.read().clone())
                    .collect::<ArgumentList>(),
            ),
        }
    }

    fn tag_expression(tag: VariantTag) -> Result<Expression> {
        Ok(Expression::integer(tag.get()).convert(Identifier::parse("toUInt")?))
    }
}
