use askama::Template;
use boltffi_binding::{
    DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, ExportedMethodDecl,
    FieldKey, Native, NativeSymbol, RecordDecl, TypeRef,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Diagnostic, Emitted, Error, RenderContext, Result},
    target::swift::{
        SwiftHost,
        codec::{ReadExpression, Reader, ValueScope, WriteStatement, Writer},
        default_value::DefaultExpression,
        name_style::Name,
        primitive::SwiftPrimitive,
        render::{
            Documentation, SwiftType,
            function::{
                AssociatedFunction, AssociatedFunctions, Initializer, Receiver, ValueFunctions,
                ValueType,
            },
        },
        syntax::{ArgumentList, Expression, Identifier, ParameterList, Statement, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/record.swift", escape = "none")]
struct RecordTemplate<'a> {
    record: &'a Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    documentation: Documentation,
    name: TypeName,
    conforms_to_error: bool,
    body: RecordBody,
    fields: Vec<Field>,
    initializers: Vec<Initializer>,
    static_methods: Vec<AssociatedFunction>,
    instance_methods: Vec<AssociatedFunction>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    documentation: Documentation,
    name: Identifier,
    ty: TypeName,
    default: Option<Expression>,
    body: FieldBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordBody {
    Direct {
        c_type: TypeName,
        codec_payload: bool,
    },
    Encoded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FieldBody {
    Direct {
        c_name: Identifier,
        read: Expression,
        write: Statement,
    },
    Encoded {
        read: Expression,
        write: Statement,
    },
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, bridge, context),
            RecordDecl::Encoded(record) => Self::from_encoded(record, bridge, context),
            _ => Err(SwiftHost::unsupported("unknown record declaration")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = RecordTemplate { record: self }.render()?;
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

    fn direct(&self) -> bool {
        matches!(self.body, RecordBody::Direct { .. })
    }

    fn c_type(&self) -> &TypeName {
        match &self.body {
            RecordBody::Direct { c_type, .. } => c_type,
            RecordBody::Encoded => unreachable!(),
        }
    }

    fn codec_payload(&self) -> bool {
        self.body.codec_payload()
    }

    fn fields(&self) -> &[Field] {
        &self.fields
    }

    fn parameter_list(&self) -> String {
        ParameterList::new(self.fields.iter().map(Field::signature)).render("        ", "    ")
    }

    fn c_initializer_arguments(&self) -> String {
        self.fields
            .iter()
            .map(Field::c_initializer_argument)
            .collect::<ArgumentList>()
            .render("            ", "        ")
    }

    fn c_value_arguments(&self) -> String {
        self.fields
            .iter()
            .map(Field::c_value_argument)
            .collect::<ArgumentList>()
            .render("            ", "        ")
    }

    fn decode_arguments(&self) -> String {
        self.fields
            .iter()
            .map(Field::decode_argument)
            .collect::<ArgumentList>()
            .render("            ", "        ")
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

    fn from_direct(
        record: &DirectRecordDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(record.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: SwiftHost::TARGET,
                    invariant: "missing direct record C type for Swift record",
                })?;
        if record.fields().len() != c_record.fields().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "direct record field count mismatch",
            });
        }
        let (mut initializers, mut diagnostics) =
            Initializer::from_record_declarations(record.initializers(), bridge, context)?
                .into_parts();
        let (value_initializers, static_methods, static_diagnostics) = Self::value_methods(
            record.methods(),
            ValueType::record(record.id()),
            bridge,
            context,
        )?
        .into_parts();
        let (instance_methods, instance_diagnostics) =
            Self::methods(record.methods(), Some(Receiver::direct()), bridge, context)?
                .into_parts();
        initializers.extend(value_initializers);
        diagnostics.extend(static_diagnostics);
        diagnostics.extend(instance_diagnostics);
        Ok(Self {
            documentation: Documentation::new(record.meta().doc(), ""),
            name: Name::new(record.name()).type_name(),
            conforms_to_error: record.is_error_payload(),
            body: RecordBody::Direct {
                c_type: TypeName::new(c_record.name()),
                codec_payload: record.is_codec_payload(),
            },
            fields: record
                .fields()
                .iter()
                .zip(c_record.fields())
                .map(|(field, c_field)| Field::from_direct(field, c_field.name()))
                .collect::<Result<Vec<_>>>()?,
            initializers,
            static_methods,
            instance_methods,
            diagnostics,
        })
    }

    fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let (mut initializers, mut diagnostics) =
            Initializer::from_record_declarations(record.initializers(), bridge, context)?
                .into_parts();
        let (value_initializers, static_methods, static_diagnostics) = Self::value_methods(
            record.methods(),
            ValueType::record(record.id()),
            bridge,
            context,
        )?
        .into_parts();
        let (instance_methods, instance_diagnostics) = Self::methods(
            record.methods(),
            Some(Receiver::encoded(
                record.name(),
                record.read(),
                record.write(),
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
            documentation: Documentation::new(record.meta().doc(), ""),
            name: Name::new(record.name()).type_name(),
            conforms_to_error: record.is_error_payload(),
            body: RecordBody::Encoded,
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_encoded(field, context, &reader, &writer))
                .collect::<Result<Vec<_>>>()?,
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

impl RecordBody {
    fn requires_wire_runtime(&self) -> bool {
        self.codec_payload()
    }

    fn codec_payload(&self) -> bool {
        match self {
            Self::Direct { codec_payload, .. } => *codec_payload,
            Self::Encoded => true,
        }
    }
}

impl Field {
    fn from_direct(field: &DirectFieldDecl, c_name: &str) -> Result<Self> {
        let name = Self::field_name(field.key())?;
        let primitive = SwiftPrimitive::new(field.ty().primitive());
        Ok(Self {
            documentation: Documentation::new(field.meta().doc(), "    "),
            name: name.clone(),
            ty: SwiftType::primitive(field.ty().primitive())?,
            default: field
                .meta()
                .default()
                .map(|default| {
                    DefaultExpression::render(&TypeRef::Primitive(field.ty().primitive()), default)
                })
                .transpose()?,
            body: FieldBody::Direct {
                c_name: Identifier::parse(c_name)?,
                read: primitive.read_expression(Identifier::parse("reader")?)?,
                write: primitive.write_statement(
                    Identifier::parse("writer")?,
                    Expression::member("self", name),
                )?,
            },
        })
    }

    fn from_encoded(
        field: &EncodedFieldDecl,
        context: &RenderContext<Native>,
        reader: &Identifier,
        writer: &Identifier,
    ) -> Result<Self> {
        let name = Self::field_name(field.key())?;
        let read = field
            .read()
            .render_with(&mut Reader::new(reader.clone(), context))
            .map(ReadExpression::into_expression)?;
        let write = field
            .write()
            .render_with(&mut Writer::new(
                writer.clone(),
                ValueScope::record(Expression::new("self")),
                context,
            ))
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(WriteStatement::into_statement)
            .collect::<Vec<_>>();
        match write.as_slice() {
            [write] => Ok(Self {
                documentation: Documentation::new(field.meta().doc(), "    "),
                name,
                ty: SwiftType::type_ref(field.ty(), context)?,
                default: field
                    .meta()
                    .default()
                    .map(|default| DefaultExpression::render(field.ty(), default))
                    .transpose()?,
                body: FieldBody::Encoded {
                    read,
                    write: write.clone(),
                },
            }),
            _ => Err(SwiftHost::unsupported(
                "multi-statement encoded record field",
            )),
        }
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn signature(&self) -> String {
        match &self.default {
            Some(default) => format!("{}: {} = {}", self.name, self.ty, default),
            None => format!("{}: {}", self.name, self.ty),
        }
    }

    fn assignment(&self) -> Expression {
        Expression::new(format!("self.{} = {}", self.name, self.name))
    }

    fn read(&self) -> &Expression {
        match &self.body {
            FieldBody::Direct { read, .. } | FieldBody::Encoded { read, .. } => read,
        }
    }

    fn write(&self) -> &Statement {
        match &self.body {
            FieldBody::Direct { write, .. } | FieldBody::Encoded { write, .. } => write,
        }
    }

    fn c_initializer_argument(&self) -> Expression {
        match &self.body {
            FieldBody::Direct { c_name, .. } => {
                Expression::labeled(&self.name, Expression::member("c", c_name))
            }
            FieldBody::Encoded { .. } => unreachable!(),
        }
    }

    fn c_value_argument(&self) -> Expression {
        match &self.body {
            FieldBody::Direct { c_name, .. } => Expression::labeled(c_name, &self.name),
            FieldBody::Encoded { .. } => unreachable!(),
        }
    }

    fn decode_argument(&self) -> Expression {
        Expression::labeled(&self.name, self.read())
    }

    fn field_name(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).field(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(SwiftHost::unsupported("unknown record field key")),
        }
    }
}
