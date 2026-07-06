use askama::Template;
use boltffi_binding::{
    CStyleEnumDecl, CStyleVariantDecl, DataEnumDecl, DataVariantDecl, DataVariantPayload,
    EncodedFieldDecl, EnumDecl, ExportedMethodDecl, FieldKey, Native, NativeSymbol,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Diagnostic, Emitted, RenderContext, Result},
    target::swift::{
        SwiftHost,
        codec::{ReadExpression, Reader, ValueScope, WriteStatement, Writer},
        name_style::Name,
        render::{
            Documentation, SwiftType,
            function::{
                AssociatedFunction, AssociatedFunctions, Initializer, Receiver, ValueFunctions,
                ValueType,
            },
        },
        syntax::{Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/enumeration.swift", escape = "none")]
struct EnumerationTemplate<'a> {
    enumeration: &'a Enumeration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enumeration {
    documentation: Documentation,
    name: TypeName,
    conforms_to_error: bool,
    body: Body,
    initializers: Vec<Initializer>,
    static_methods: Vec<AssociatedFunction>,
    instance_methods: Vec<AssociatedFunction>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    CStyle {
        raw_type: TypeName,
        variants: Vec<CStyleVariant>,
    },
    Data {
        variants: Vec<DataVariant>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CStyleVariant {
    documentation: Documentation,
    name: Identifier,
    discriminant: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataVariant {
    documentation: Documentation,
    name: Identifier,
    tag: u32,
    payload: Payload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Payload {
    Unit,
    Tuple(Vec<DataField>),
    Struct(Vec<DataField>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataField {
    name: Identifier,
    ty: TypeName,
    read: Expression,
    write: Statement,
}

impl Enumeration {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, bridge, context),
            EnumDecl::Data(enumeration) => Self::from_data(enumeration, bridge, context),
            _ => Err(SwiftHost::unsupported("unknown enum declaration")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = EnumerationTemplate { enumeration: self }.render()?;
        source.push_str("\n\n");
        let emitted = Emitted::primary(source).with_diagnostics(self.diagnostics.clone());
        let emitted = match self.requires_wire_runtime() {
            true => emitted.with_aux(AssociatedFunction::wire_helper()?),
            false => emitted,
        };
        let emitted = match self.requires_async_runtime() {
            true => emitted.with_aux(AssociatedFunction::async_helper()?),
            false => emitted,
        };
        Ok(emitted)
    }

    fn name(&self) -> &TypeName {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn error_conformance(&self) -> &str {
        match self.conforms_to_error {
            true => ", Error",
            false => "",
        }
    }

    fn c_style(&self) -> bool {
        matches!(self.body, Body::CStyle { .. })
    }

    fn data(&self) -> bool {
        matches!(self.body, Body::Data { .. })
    }

    fn raw_type(&self) -> &TypeName {
        match &self.body {
            Body::CStyle { raw_type, .. } => raw_type,
            Body::Data { .. } => unreachable!(),
        }
    }

    fn c_style_variants(&self) -> &[CStyleVariant] {
        match &self.body {
            Body::CStyle { variants, .. } => variants,
            Body::Data { .. } => &[],
        }
    }

    fn data_variants(&self) -> &[DataVariant] {
        match &self.body {
            Body::Data { variants } => variants,
            Body::CStyle { .. } => &[],
        }
    }

    fn initializers(&self) -> &[Initializer] {
        &self.initializers
    }

    fn static_methods(&self) -> &[AssociatedFunction] {
        &self.static_methods
    }

    fn instance_methods(&self) -> &[AssociatedFunction] {
        &self.instance_methods
    }

    fn requires_wire_runtime(&self) -> bool {
        self.body.requires_wire_runtime()
            || self
                .initializers
                .iter()
                .any(Initializer::requires_wire_runtime)
            || self
                .static_methods
                .iter()
                .chain(self.instance_methods.iter())
                .any(AssociatedFunction::requires_wire_runtime)
    }

    fn requires_async_runtime(&self) -> bool {
        self.static_methods
            .iter()
            .chain(self.instance_methods.iter())
            .any(AssociatedFunction::requires_async_runtime)
    }

    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let (mut initializers, mut diagnostics) =
            Initializer::from_enum_declarations(enumeration.initializers(), bridge, context)?
                .into_parts();
        let (value_initializers, static_methods, static_diagnostics) = Self::value_methods(
            enumeration.methods(),
            ValueType::enumeration(enumeration.id()),
            bridge,
            context,
        )?
        .into_parts();
        let (instance_methods, instance_diagnostics) = Self::methods(
            enumeration.methods(),
            Some(Receiver::direct()),
            bridge,
            context,
        )?
        .into_parts();
        initializers.extend(value_initializers);
        diagnostics.extend(static_diagnostics);
        diagnostics.extend(instance_diagnostics);
        Ok(Self {
            documentation: Documentation::new(enumeration.meta().doc(), ""),
            name: Name::new(enumeration.name()).type_name(),
            conforms_to_error: enumeration.is_error_payload(),
            body: Body::CStyle {
                raw_type: SwiftType::primitive(enumeration.repr().primitive())?,
                variants: enumeration
                    .variants()
                    .iter()
                    .map(CStyleVariant::from_declaration)
                    .collect::<Result<Vec<_>>>()?,
            },
            initializers,
            static_methods,
            instance_methods,
            diagnostics,
        })
    }

    fn from_data(
        enumeration: &DataEnumDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let (mut initializers, mut diagnostics) =
            Initializer::from_enum_declarations(enumeration.initializers(), bridge, context)?
                .into_parts();
        let (value_initializers, static_methods, static_diagnostics) = Self::value_methods(
            enumeration.methods(),
            ValueType::enumeration(enumeration.id()),
            bridge,
            context,
        )?
        .into_parts();
        let (instance_methods, instance_diagnostics) = Self::methods(
            enumeration.methods(),
            Some(Receiver::encoded(
                enumeration.name(),
                enumeration.read(),
                enumeration.write(),
                bridge,
                context,
            )?),
            bridge,
            context,
        )?
        .into_parts();
        initializers.extend(value_initializers);
        diagnostics.extend(static_diagnostics);
        diagnostics.extend(instance_diagnostics);
        Ok(Self {
            documentation: Documentation::new(enumeration.meta().doc(), ""),
            name: Name::new(enumeration.name()).type_name(),
            conforms_to_error: enumeration.is_error_payload(),
            body: Body::Data {
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| DataVariant::from_declaration(variant, context))
                    .collect::<Result<Vec<_>>>()?,
            },
            initializers,
            static_methods,
            instance_methods,
            diagnostics,
        })
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<AssociatedFunctions> {
        AssociatedFunction::from_methods(methods, receiver, bridge, context)
    }

    fn value_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        value_type: ValueType,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<ValueFunctions> {
        AssociatedFunction::from_value_methods(methods, value_type, None, bridge, context)
    }
}

impl Body {
    fn requires_wire_runtime(&self) -> bool {
        matches!(self, Self::Data { .. })
    }
}

impl CStyleVariant {
    fn from_declaration(variant: &CStyleVariantDecl) -> Result<Self> {
        Ok(Self {
            documentation: Documentation::new(variant.meta().doc(), "    "),
            name: Name::new(variant.name()).variant()?,
            discriminant: variant.discriminant().get(),
        })
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    const fn discriminant(&self) -> i128 {
        self.discriminant
    }
}

impl DataVariant {
    fn from_declaration(
        variant: &DataVariantDecl,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            documentation: Documentation::new(variant.meta().doc(), "    "),
            name: Name::new(variant.name()).variant()?,
            tag: variant.tag().get(),
            payload: Payload::from_declaration(variant.payload(), context)?,
        })
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    const fn tag(&self) -> u32 {
        self.tag
    }

    fn payload(&self) -> &Payload {
        &self.payload
    }
}

impl Payload {
    fn from_declaration(
        payload: &DataVariantPayload,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match payload {
            DataVariantPayload::Unit => Ok(Self::Unit),
            DataVariantPayload::Tuple(fields) => fields
                .iter()
                .map(|field| DataField::from_declaration(field, context))
                .collect::<Result<Vec<_>>>()
                .map(Self::Tuple),
            DataVariantPayload::Struct(fields) => fields
                .iter()
                .map(|field| DataField::from_declaration(field, context))
                .collect::<Result<Vec<_>>>()
                .map(Self::Struct),
            _ => Err(SwiftHost::unsupported("unknown data enum payload")),
        }
    }

    fn unit(&self) -> bool {
        matches!(self, Self::Unit)
    }

    fn fields(&self) -> &[DataField] {
        match self {
            Self::Unit => &[],
            Self::Tuple(fields) | Self::Struct(fields) => fields,
        }
    }

    fn associated_values(&self) -> String {
        match self {
            Self::Unit => String::new(),
            Self::Tuple(fields) if fields.len() == 1 => format!("({})", fields[0].ty()),
            Self::Tuple(fields) | Self::Struct(fields) => format!(
                "({})",
                fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name(), field.ty()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    fn read_arguments(&self) -> String {
        match self {
            Self::Unit => String::new(),
            Self::Tuple(fields) if fields.len() == 1 => format!("({})", fields[0].read()),
            Self::Tuple(fields) | Self::Struct(fields) => format!(
                "({})",
                fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name(), field.read()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    fn case_pattern(&self) -> String {
        match self {
            Self::Unit => String::new(),
            Self::Tuple(fields) | Self::Struct(fields) => format!(
                "({})",
                fields
                    .iter()
                    .map(DataField::name)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl DataField {
    fn from_declaration(field: &EncodedFieldDecl, context: &RenderContext<Native>) -> Result<Self> {
        let name = Self::field_name(field.key())?;
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let scope = ValueScope::fields(vec![(
            field.key().clone(),
            Expression::identifier(name.clone()),
        )]);
        let write = field
            .write()
            .render_with(&mut Writer::new(writer, scope, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        match write.as_slice() {
            [write] => Ok(Self {
                name,
                ty: SwiftType::type_ref(field.ty(), context)?,
                read: field
                    .read()
                    .render_with(&mut Reader::new(reader, context))
                    .map(ReadExpression::into_expression)?,
                write: write.clone(),
            }),
            _ => Err(SwiftHost::unsupported("multi-statement data enum field")),
        }
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn read(&self) -> &Expression {
        &self.read
    }

    fn write(&self) -> &Statement {
        &self.write
    }

    fn field_name(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).field(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(SwiftHost::unsupported("unknown data enum field key")),
        }
    }
}
