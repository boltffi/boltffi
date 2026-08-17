use askama::Template;
use boltffi_binding::{
    ConstantOwner, DirectRecordDecl, DirectValueType, EncodedRecordDecl, Native, RecordDecl,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::dart::syntax::{Identifier, TypeFragment},
};

use super::super::{
    codec::{Reader, Sizer, ValueScope, Writer, primitive_read_method, primitive_write_method},
    default_value::DefaultExpression,
    native::NativeType,
    type_name,
    value_semantics::ValueSemantics,
};
use super::function::{Placement, Receiver, associated_functions};
use super::{
    AssociatedConstants, Documentation, Function, declaration_name, field_name, indent,
    primitive_annotation,
};

#[derive(Template)]
#[template(path = "target/dart/record.dart", escape = "none")]
struct RecordTemplate<'a> {
    record: &'a Record,
}

pub struct Record {
    documentation: Documentation,
    name: Identifier,
    implements_exception: bool,
    fields: Vec<Field>,
    members: Vec<String>,
    native: Option<NativeRecord>,
}

struct Field {
    name: Identifier,
    ty: TypeFragment,
    default: Option<DefaultExpression>,
    documentation: Documentation,
    read: String,
    writes: Vec<String>,
    size: String,
    equality: String,
    hash: String,
}

struct NativeRecord {
    name: Identifier,
    fields: Vec<NativeField>,
}

struct NativeField {
    annotation: String,
    ty: TypeFragment,
    name: Identifier,
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
            _ => Err(Error::UnexpectedBindingShape {
                layer: "dart record",
                shape: "unknown record declaration",
            }),
        }
    }

    fn from_direct(
        declaration: &DirectRecordDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "Dart direct record is missing from the C bridge",
                })?;
        if declaration.fields().len() != c_record.fields().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart direct-record field count disagrees with the C bridge",
            });
        }

        Self::new(
            declaration.meta().doc(),
            declaration_name(declaration.name())?,
            declaration.is_error_payload(),
            declaration
                .fields()
                .iter()
                .map(|field| {
                    let name = field_name(field.key())?;
                    let primitive = field.ty().primitive();
                    let semantics = ValueSemantics::direct();
                    Ok(Field {
                        equality: semantics.equality(name.as_str(), &format!("other.{name}")),
                        hash: semantics.hash(name.as_str()),
                        name,
                        ty: type_name::primitive_type(primitive)?,
                        default: field
                            .meta()
                            .default()
                            .map(|value| {
                                DefaultExpression::render(
                                    &boltffi_binding::TypeRef::Primitive(primitive),
                                    value,
                                    context,
                                )
                            })
                            .transpose()?,
                        documentation: Documentation::new(field.meta().doc(), 2),
                        read: format!("_p$reader.{}()", primitive_read_method(primitive)),
                        writes: vec![format!(
                            "_p$writer.{}({});",
                            primitive_write_method(primitive),
                            field_name(field.key())?,
                        )],
                        size: super::super::codec::primitive_size(primitive).to_string(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            associated_functions(
                declaration.initializers(),
                declaration.methods(),
                Placement::Static,
                Receiver::DirectValue(DirectValueType::Record(declaration.id())),
                bridge,
                context,
            )?,
            AssociatedConstants::from_owner(
                ConstantOwner::Record(declaration.id()),
                bridge,
                context,
            )?,
            Some(NativeRecord {
                name: Identifier::parse(format!("_$${}", c_record.name()))?,
                fields: declaration
                    .fields()
                    .iter()
                    .map(|field| {
                        let primitive = field.ty().primitive();
                        Ok(NativeField {
                            annotation: primitive_annotation(primitive)?,
                            ty: TypeFragment::new(NativeType::primitive(primitive)?.dart()),
                            name: field_name(field.key())?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            }),
        )
    }

    fn from_encoded(
        declaration: &EncodedRecordDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let scope = ValueScope::fields(
            declaration
                .fields()
                .iter()
                .map(|field| Ok((field.key().clone(), field_name(field.key())?.to_string())))
                .collect::<Result<Vec<_>>>()?,
        );
        Self::new(
            declaration.meta().doc(),
            declaration_name(declaration.name())?,
            declaration.is_error_payload(),
            declaration
                .fields()
                .iter()
                .map(|field| {
                    let name = field_name(field.key())?;
                    let semantics = ValueSemantics::for_type(field.ty())?;
                    Ok(Field {
                        equality: semantics.equality(name.as_str(), &format!("other.{name}")),
                        hash: semantics.hash(name.as_str()),
                        name,
                        ty: type_name::type_ref(field.ty(), context)?,
                        default: field
                            .meta()
                            .default()
                            .map(|value| DefaultExpression::render(field.ty(), value, context))
                            .transpose()?,
                        documentation: Documentation::new(field.meta().doc(), 2),
                        read: field
                            .read()
                            .render_with(&mut Reader::new("_p$reader", context))?
                            .into_source(),
                        writes: field
                            .write()
                            .render_with(&mut Writer::new("_p$writer", scope.clone(), context))
                            .into_iter()
                            .collect::<Result<Vec<_>>>()?
                            .into_iter()
                            .map(super::super::codec::WriteStatement::into_source)
                            .collect(),
                        size: field
                            .write()
                            .size_with(&mut Sizer::new(scope.clone(), context))?
                            .into_source(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            associated_functions(
                declaration.initializers(),
                declaration.methods(),
                Placement::Static,
                Receiver::EncodedValue,
                bridge,
                context,
            )?,
            AssociatedConstants::from_owner(
                ConstantOwner::Record(declaration.id()),
                bridge,
                context,
            )?,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        doc: Option<&boltffi_binding::DocComment>,
        name: Identifier,
        implements_exception: bool,
        fields: Vec<Field>,
        methods: Vec<Function>,
        constants: AssociatedConstants,
        native: Option<NativeRecord>,
    ) -> Result<Self> {
        let members = constants
            .iter()
            .map(|constant| indent(constant.source(), 2))
            .chain(methods.iter().map(|method| indent(&method.source(), 2)))
            .collect();
        Ok(Self {
            documentation: Documentation::new(doc, 0),
            name,
            implements_exception,
            fields,
            members,
            native,
        })
    }

    pub fn render(self) -> Emitted {
        Emitted::primary(
            RecordTemplate { record: &self }
                .render()
                .expect("rendering an in-memory Dart record template cannot fail"),
        )
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn exception_clause(&self) -> &'static str {
        if self.implements_exception {
            " implements Exception"
        } else {
            ""
        }
    }

    fn fields(&self) -> &[Field] {
        &self.fields
    }

    fn members(&self) -> &[String] {
        &self.members
    }

    fn native(&self) -> Option<&NativeRecord> {
        self.native.as_ref()
    }

    fn encoded_size(&self) -> String {
        if self.fields.is_empty() {
            "0".to_owned()
        } else {
            self.fields
                .iter()
                .map(Field::size)
                .collect::<Vec<_>>()
                .join(" + ")
        }
    }

    fn default_initializers(&self) -> String {
        let initializers = self
            .fields
            .iter()
            .filter_map(Field::default_initializer)
            .collect::<Vec<_>>();
        match initializers.as_slice() {
            [] => String::new(),
            _ => format!(" :\n      {}", initializers.join(",\n      ")),
        }
    }
}

impl Field {
    fn name(&self) -> &Identifier {
        &self.name
    }

    fn ty(&self) -> &TypeFragment {
        &self.ty
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn default_clause(&self) -> String {
        self.default.as_ref().map_or_else(
            || format!("required this.{}", self.name),
            |default| match default {
                DefaultExpression::Constant(default) => {
                    format!("this.{} = {default}", self.name)
                }
                DefaultExpression::Runtime(_) => format!("{}? {}", self.ty, self.name),
            },
        )
    }

    fn default_initializer(&self) -> Option<String> {
        match &self.default {
            Some(DefaultExpression::Runtime(default)) => {
                Some(format!("{} = {} ?? {default}", self.name, self.name))
            }
            Some(DefaultExpression::Constant(_)) | None => None,
        }
    }

    fn read(&self) -> &str {
        &self.read
    }

    fn writes(&self) -> &[String] {
        &self.writes
    }

    fn size(&self) -> &str {
        &self.size
    }

    fn equality(&self) -> &str {
        &self.equality
    }

    fn hash(&self) -> &str {
        &self.hash
    }
}

impl NativeRecord {
    fn name(&self) -> &Identifier {
        &self.name
    }

    fn fields(&self) -> &[NativeField] {
        &self.fields
    }
}

impl NativeField {
    fn annotation(&self) -> &str {
        &self.annotation
    }

    fn ty(&self) -> &TypeFragment {
        &self.ty
    }

    fn name(&self) -> &Identifier {
        &self.name
    }
}
