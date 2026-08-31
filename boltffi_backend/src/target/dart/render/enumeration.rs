use askama::Template;
use boltffi_binding::{
    CStyleEnumDecl, DataEnumDecl, DataVariantPayload, DirectValueType, EnumDecl, Native,
};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::dart::syntax::{Identifier, TypeFragment},
};

use super::super::{
    codec::{Reader, Sizer, ValueScope, Writer, primitive_read_method, primitive_write_method},
    name_style::Name,
    type_name,
    value_semantics::ValueSemantics,
};
use super::function::{Placement, Receiver, associated_functions};
use super::{AssociatedConstants, Documentation, declaration_name, field_name, indent};

#[derive(Template)]
#[template(path = "target/dart/enumeration.dart", escape = "none")]
struct EnumerationTemplate<'a> {
    enumeration: &'a Enumeration,
}

pub struct Enumeration {
    documentation: Documentation,
    name: Identifier,
    implements_exception: bool,
    body: Body,
    members: Vec<String>,
}

enum Body {
    CStyle(CStyleBody),
    Data(DataBody),
}

struct CStyleBody {
    variants: Vec<CStyleVariant>,
    read_method: &'static str,
    write_method: &'static str,
    encoded_size: usize,
}

struct CStyleVariant {
    documentation: Documentation,
    name: Identifier,
    discriminant: i128,
}

struct DataBody {
    variants: Vec<DataVariant>,
}

struct DataVariant {
    declaration_documentation: Documentation,
    member_documentation: Documentation,
    name: Identifier,
    class_name: Identifier,
    tag: u32,
    fields: Vec<DataField>,
}

struct DataField {
    name: Identifier,
    ty: TypeFragment,
    read: String,
    writes: Vec<String>,
    size: String,
    equality: String,
    hash: String,
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
            _ => Err(Error::UnexpectedBindingShape {
                layer: "dart enum",
                shape: "unknown enum declaration",
            }),
        }
    }

    fn from_c_style(
        declaration: &CStyleEnumDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let primitive = declaration.repr().primitive();
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), 0),
            name: declaration_name(declaration.name())?,
            implements_exception: declaration.is_error_payload(),
            body: Body::CStyle(CStyleBody {
                variants: declaration
                    .variants()
                    .iter()
                    .map(|variant| {
                        Ok(CStyleVariant {
                            documentation: Documentation::new(variant.meta().doc(), 2),
                            name: Name::new(variant.name()).lower_camel()?,
                            discriminant: variant.discriminant().get(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
                read_method: primitive_read_method(primitive),
                write_method: primitive_write_method(primitive),
                encoded_size: super::super::codec::primitive_size(primitive),
            }),
            members: associated_members(
                AssociatedConstants::from_enum(declaration.id(), bridge, context)?,
                associated_functions(
                    declaration.initializers(),
                    declaration.methods(),
                    Placement::Static,
                    Receiver::DirectValue(DirectValueType::Enum(declaration.id())),
                    bridge,
                    context,
                )?,
            ),
        })
    }

    fn from_data(
        declaration: &DataEnumDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if declaration.has_transparent_variants() {
            return super::super::unsupported("transparent data enum variant");
        }
        let name = declaration_name(declaration.name())?;
        Ok(Self {
            documentation: Documentation::new(declaration.meta().doc(), 0),
            name: name.clone(),
            implements_exception: declaration.is_error_payload(),
            body: Body::Data(DataBody {
                variants: declaration
                    .variants()
                    .iter()
                    .map(|variant| DataVariant::from_declaration(&name, variant, context))
                    .collect::<Result<Vec<_>>>()?,
            }),
            members: associated_members(
                AssociatedConstants::from_enum(declaration.id(), bridge, context)?,
                associated_functions(
                    declaration.initializers(),
                    declaration.methods(),
                    Placement::Static,
                    Receiver::EncodedValue,
                    bridge,
                    context,
                )?,
            ),
        })
    }

    pub fn render(self) -> Emitted {
        Emitted::primary(
            EnumerationTemplate { enumeration: &self }
                .render()
                .expect("rendering an in-memory Dart enum template cannot fail"),
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

    fn c_style(&self) -> bool {
        matches!(self.body, Body::CStyle(_))
    }

    fn data(&self) -> bool {
        matches!(self.body, Body::Data(_))
    }

    fn c_style_body(&self) -> &CStyleBody {
        match &self.body {
            Body::CStyle(body) => body,
            Body::Data(_) => unreachable!("template guards C-style enum access"),
        }
    }

    fn data_body(&self) -> &DataBody {
        match &self.body {
            Body::Data(body) => body,
            Body::CStyle(_) => unreachable!("template guards data enum access"),
        }
    }

    fn members(&self) -> &[String] {
        &self.members
    }
}

impl CStyleBody {
    fn variants(&self) -> &[CStyleVariant] {
        &self.variants
    }

    fn read_method(&self) -> &str {
        self.read_method
    }

    fn write_method(&self) -> &str {
        self.write_method
    }

    fn encoded_size(&self) -> usize {
        self.encoded_size
    }
}

impl CStyleVariant {
    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn discriminant(&self) -> i128 {
        self.discriminant
    }
}

impl DataBody {
    fn variants(&self) -> &[DataVariant] {
        &self.variants
    }
}

impl DataVariant {
    fn from_declaration(
        enum_name: &Identifier,
        variant: &boltffi_binding::DataVariantDecl,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let field_declarations = match variant.payload() {
            DataVariantPayload::Unit => &[][..],
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => {
                fields.as_slice()
            }
            _ => return super::super::unsupported("unknown Dart data-enum payload"),
        };
        let scope = ValueScope::fields(
            field_declarations
                .iter()
                .map(|field| Ok((field.key().clone(), field_name(field.key())?.to_string())))
                .collect::<Result<Vec<_>>>()?,
        );
        Ok(Self {
            declaration_documentation: Documentation::new(variant.meta().doc(), 0),
            member_documentation: Documentation::new(variant.meta().doc(), 2),
            name: Name::new(variant.name()).lower_camel()?,
            class_name: Identifier::parse(format!(
                "{enum_name}${}",
                Name::new(variant.name()).upper_camel()?
            ))?,
            tag: variant.tag().get(),
            fields: field_declarations
                .iter()
                .map(|field| {
                    let name = field_name(field.key())?;
                    let semantics = ValueSemantics::for_type(field.ty())?;
                    Ok(DataField {
                        equality: semantics.equality(name.as_str(), &format!("other.{name}")),
                        hash: semantics.hash(name.as_str()),
                        name,
                        ty: type_name::type_ref(field.ty(), context)?,
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
        })
    }

    fn declaration_documentation(&self) -> &Documentation {
        &self.declaration_documentation
    }

    fn member_documentation(&self) -> &Documentation {
        &self.member_documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn class_name(&self) -> &Identifier {
        &self.class_name
    }

    fn tag(&self) -> u32 {
        self.tag
    }

    fn fields(&self) -> &[DataField] {
        &self.fields
    }

    fn unit(&self) -> bool {
        self.fields.is_empty()
    }

    fn encoded_size(&self) -> String {
        std::iter::once("4")
            .chain(self.fields.iter().map(|field| field.size()))
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

impl DataField {
    fn name(&self) -> &Identifier {
        &self.name
    }

    fn ty(&self) -> &TypeFragment {
        &self.ty
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

fn associated_members(
    constants: AssociatedConstants,
    methods: Vec<super::Function>,
) -> Vec<String> {
    constants
        .iter()
        .map(|constant| indent(constant.source(), 2))
        .chain(methods.iter().map(|method| indent(&method.source(), 2)))
        .collect()
}
