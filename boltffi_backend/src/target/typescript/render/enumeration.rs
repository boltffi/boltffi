use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, DataEnumDecl, DataVariantDecl, DataVariantPayload, EncodedFieldDecl, EnumDecl,
    IntegerRepr, Wasm32,
};

use crate::core::{Diagnostic, Emitted, Error, RenderContext, Result};

use super::super::{
    codec::{Reader, Sizer, Writer},
    name_style::Name,
    primitive::Scalar,
    syntax::{
        Expression, Identifier, IntegerLiteral, MethodDeclaration, PropertyKey, Statement,
        StringLiteral, TypeName,
    },
};
use super::{Function, Type};

pub enum Enumeration {
    CStyle(CStyle),
    Data(Data),
}

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/c_style_enum.ts", escape = "none")]
pub struct CStyle {
    name: TypeName,
    codec: Identifier,
    variants: Vec<CStyleVariant>,
    size: u64,
    write: Identifier,
    read: Identifier,
    methods: Vec<MethodDeclaration>,
    diagnostics: Vec<Diagnostic>,
    error: bool,
}

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/data_enum.ts", escape = "none")]
pub struct Data {
    name: TypeName,
    codec: Identifier,
    variants: Vec<DataVariant>,
    methods: Vec<MethodDeclaration>,
    diagnostics: Vec<Diagnostic>,
    error: bool,
}

struct CStyleVariant {
    name: Identifier,
    value: IntegerLiteral,
}

struct DataVariant {
    tag: StringLiteral,
    wire_tag: u32,
    fields: Vec<DataField>,
    size: Expression,
    writes: Vec<Statement>,
    reads: Vec<DataRead>,
}

struct DataField {
    key: PropertyKey,
    ty: TypeName,
}

struct DataRead {
    key: PropertyKey,
    value: Expression,
}

impl Enumeration {
    pub fn from_declaration(
        declaration: &EnumDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => CStyle::new(enumeration, context).map(Self::CStyle),
            EnumDecl::Data(enumeration) => Data::new(enumeration, context).map(Self::Data),
            _ => Err(Self::error("unknown enum declaration")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        match self {
            Self::CStyle(enumeration) => enumeration.render(),
            Self::Data(enumeration) => enumeration.render(),
        }
    }

    fn error(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl CStyle {
    fn new(enumeration: &CStyleEnumDecl<Wasm32>, context: &RenderContext<Wasm32>) -> Result<Self> {
        let primitive = enumeration.repr().primitive();
        let scalar = Scalar::new(primitive)?;
        let bigint = matches!(enumeration.repr(), IntegerRepr::I64 | IntegerRepr::U64);
        let (methods, diagnostics) = Function::c_style_enum_methods(
            enumeration.id(),
            enumeration.initializers(),
            enumeration.methods(),
            context,
        )?;
        Ok(Self {
            name: Name::new(enumeration.name()).type_name(),
            codec: Name::new(enumeration.name()).codec_identifier()?,
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| {
                    Ok(CStyleVariant {
                        name: Name::new(variant.name()).variant_identifier()?,
                        value: match bigint {
                            true => IntegerLiteral::bigint(variant.discriminant().get()),
                            false => IntegerLiteral::number(variant.discriminant().get()),
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            size: primitive.byte_size::<Wasm32>().get(),
            write: scalar.write_method(),
            read: scalar.read_method(),
            methods,
            diagnostics,
            error: enumeration.is_error_payload(),
        })
    }

    fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?)
            .with_diagnostics(self.diagnostics.clone()))
    }
}

impl Data {
    fn new(enumeration: &DataEnumDecl<Wasm32>, context: &RenderContext<Wasm32>) -> Result<Self> {
        let (methods, diagnostics) = Function::data_enum_methods(
            enumeration.id(),
            enumeration.initializers(),
            enumeration.methods(),
            context,
        )?;
        Ok(Self {
            name: Name::new(enumeration.name()).type_name(),
            codec: Name::new(enumeration.name()).codec_identifier()?,
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| DataVariant::new(variant, context))
                .collect::<Result<Vec<_>>>()?,
            methods,
            diagnostics,
            error: enumeration.is_error_payload(),
        })
    }

    fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?)
            .with_diagnostics(self.diagnostics.clone()))
    }
}

impl DataVariant {
    fn new(variant: &DataVariantDecl, context: &RenderContext<Wasm32>) -> Result<Self> {
        let fields = match variant.payload() {
            DataVariantPayload::Unit => &[][..],
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => fields,
            _ => return Err(Enumeration::error("unknown data enum payload")),
        };
        let value = Expression::identifier(Identifier::known("value"));
        let writer = Identifier::known("writer");
        let reader = Identifier::known("reader");
        let size = fields
            .iter()
            .map(|field| {
                field
                    .write()
                    .size_with(&mut Sizer::new(value.clone(), context))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|size| size.into_expression())
            .fold(Expression::integer(4), Expression::add);
        let writes = fields
            .iter()
            .flat_map(|field| {
                field
                    .write()
                    .render_with(&mut Writer::new(writer.clone(), value.clone(), context))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|write| write.into_statement())
            .collect();
        Ok(Self {
            tag: StringLiteral::new(&Name::new(variant.name()).variant_identifier()?.to_string()),
            wire_tag: variant.tag().get(),
            fields: fields
                .iter()
                .map(|field| DataField::new(field, context))
                .collect::<Result<Vec<_>>>()?,
            size,
            writes,
            reads: fields
                .iter()
                .map(|field| DataRead::new(field, &reader, context))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl DataField {
    fn new(field: &EncodedFieldDecl, context: &RenderContext<Wasm32>) -> Result<Self> {
        Ok(Self {
            key: PropertyKey::from_field(field.key())?,
            ty: Type::from_ref(field.ty(), context)?,
        })
    }
}

impl DataRead {
    fn new(
        field: &EncodedFieldDecl,
        reader: &Identifier,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        Ok(Self {
            key: PropertyKey::from_field(field.key())?,
            value: field
                .read()
                .render_with(&mut Reader::new(reader.clone(), context))?
                .into_expression(),
        })
    }
}
