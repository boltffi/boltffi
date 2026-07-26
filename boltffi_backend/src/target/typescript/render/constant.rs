use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ConstantDecl, ConstantValueDecl, DeclarationRef, DefaultValue, EnumDecl, Primitive, TypeRef,
    Wasm32,
};

use crate::core::{Emitted, Error, RenderContext, RenderedDeclaration, Result};

use super::super::{
    name_style::Name,
    syntax::{Expression, Identifier, IntegerLiteral, PropertyKey, StringLiteral, TypeName},
};
use super::{Function, Type};

#[derive(AskamaTemplate)]
#[template(path = "target/typescript/constant.ts", escape = "none")]
pub struct Constant {
    inline: Option<Inline>,
    accessor: Option<Accessor>,
}

struct Inline {
    name: Identifier,
    ty: TypeName,
    value: Expression,
}

struct Accessor {
    name: Identifier,
    ty: TypeName,
    reader: Identifier,
    function: String,
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Ok(Self {
                inline: Some(Inline {
                    name: Name::new(declaration.name()).identifier()?,
                    ty: Type::from_ref(ty, context)?,
                    value: Self::default_value(ty, value, context)?,
                }),
                accessor: None,
            }),
            ConstantValueDecl::Accessor { symbol, callable } => {
                let function =
                    Function::constant_accessor(declaration.name(), symbol, callable, context)?;
                let name = Name::new(declaration.name()).identifier()?;
                let spelling = name.to_string();
                let mut characters = spelling.chars();
                let reader_suffix = characters
                    .next()
                    .map(|first| first.to_uppercase().chain(characters).collect::<String>())
                    .ok_or_else(|| Self::unsupported("constant accessor name"))?;
                let reader = Identifier::parse(format!("_read{reader_suffix}"))?;
                Ok(Self {
                    inline: None,
                    accessor: Some(Accessor {
                        name,
                        ty: function.return_type().clone(),
                        function: function.render_local(&reader)?,
                        reader,
                    }),
                })
            }
            _ => Err(Self::unsupported("constant value")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        Ok(Emitted::primary(AskamaTemplate::render(self)?))
    }

    pub fn initializers(
        declarations: &[RenderedDeclaration<'_, Wasm32>],
        context: &RenderContext<Wasm32>,
    ) -> Result<String> {
        declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Constant(constant) => Some(constant),
                _ => None,
            })
            .map(|declaration| Self::from_declaration(declaration, context))
            .filter_map(|constant| match constant {
                Ok(Self {
                    accessor: Some(accessor),
                    ..
                }) => Some(Ok(format!(
                    "  {} = {}();\n",
                    accessor.name, accessor.reader
                ))),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>>>()
            .map(|initializers| initializers.concat())
    }

    fn default_value(
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Wasm32>,
    ) -> Result<Expression> {
        match value {
            DefaultValue::Bool(value) => Ok(Expression::boolean(*value)),
            DefaultValue::Integer(value)
                if matches!(ty, TypeRef::Primitive(Primitive::I64 | Primitive::U64)) =>
            {
                Ok(Expression::integer_literal(IntegerLiteral::bigint(
                    value.get(),
                )))
            }
            DefaultValue::Integer(value) => Ok(Expression::integer_literal(
                IntegerLiteral::number(value.get()),
            )),
            DefaultValue::Float(value) => Ok(Expression::floating(value.to_f64())),
            DefaultValue::String(value) => Ok(Expression::string(StringLiteral::new(value))),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => match ty {
                TypeRef::Enum(id) => match context.enumeration(*id) {
                    Some(EnumDecl::CStyle(_)) => Ok(Expression::property(
                        Expression::identifier(Identifier::parse(
                            Name::new(enum_name).type_name().to_string(),
                        )?),
                        Name::new(variant_name).variant_identifier()?,
                    )),
                    Some(EnumDecl::Data(_)) => Ok(Expression::object([(
                        PropertyKey::Named(Identifier::known("tag")),
                        Expression::string(StringLiteral::new(
                            &Name::new(variant_name).variant_identifier()?.to_string(),
                        )),
                    )])),
                    _ => Err(Self::unsupported("constant enum declaration")),
                },
                _ => Err(Self::unsupported("constant enum type")),
            },
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(Self::unsupported("constant default value")),
        }
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}
