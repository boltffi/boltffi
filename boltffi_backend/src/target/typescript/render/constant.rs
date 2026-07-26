use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, ConstantDecl, ConstantOwner, ConstantValueDecl, DeclarationRef, DefaultValue,
    EnumDecl, IntegerRepr, Primitive, TypeRef, Wasm32,
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
    owner: Option<TypeName>,
    name: Identifier,
    body: Body,
}

pub(super) struct AssociatedConstants {
    pub(super) members: Vec<String>,
    pub(super) functions: Vec<String>,
}

#[derive(Clone, Copy)]
enum MemberStyle {
    Object,
    Class,
}

enum Body {
    Inline(Inline),
    Accessor(Accessor),
}

struct Inline {
    ty: TypeName,
    value: Expression,
}

struct Accessor {
    ty: TypeName,
    reader: Identifier,
    function: String,
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Wasm32>,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let owner = declaration
            .owner()
            .map(|owner| Self::owner_name(owner, context))
            .transpose()?;
        let name = if owner.is_some() {
            Name::new(declaration.name()).constant_identifier()?
        } else {
            Name::new(declaration.name()).identifier()?
        };
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Ok(Self {
                owner,
                name,
                body: Body::Inline(Inline {
                    ty: Type::from_ref(ty, context)?,
                    value: Self::default_value(declaration, ty, value, context)?,
                }),
            }),
            ConstantValueDecl::Accessor { symbol, callable } => {
                let function =
                    Function::constant_accessor(declaration.name(), symbol, callable, context)?;
                let reader = Identifier::parse(format!(
                    "_read{}{}",
                    owner.as_ref().map(ToString::to_string).unwrap_or_default(),
                    Name::new(declaration.name()).helper_suffix()
                ))?;
                Ok(Self {
                    owner,
                    name,
                    body: Body::Accessor(Accessor {
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
            .filter_map(|declaration| {
                Self::from_declaration(declaration, context)
                    .map(|constant| constant.initializer())
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()
            .map(|initializers| initializers.concat())
    }

    fn default_value(
        declaration: &ConstantDecl<Wasm32>,
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Wasm32>,
    ) -> Result<Expression> {
        if let (
            Some(ConstantOwner::Enum(owner)),
            TypeRef::Enum(type_id),
            DefaultValue::EnumVariant { variant_name, .. },
        ) = (declaration.owner(), ty, value)
            && owner == *type_id
            && let Some(EnumDecl::CStyle(enumeration)) = context.enumeration(owner)
        {
            return Self::c_style_variant(enumeration, variant_name);
        }
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

    fn c_style_variant(
        enumeration: &CStyleEnumDecl<Wasm32>,
        variant_name: &boltffi_binding::CanonicalName,
    ) -> Result<Expression> {
        let discriminant = enumeration
            .variants()
            .iter()
            .find(|variant| variant.name() == variant_name)
            .map(|variant| variant.discriminant().get())
            .ok_or_else(|| Self::unsupported("associated enum constant variant"))?;
        Ok(Expression::integer_literal(match enumeration.repr() {
            IntegerRepr::I64 | IntegerRepr::U64 => IntegerLiteral::bigint(discriminant),
            _ => IntegerLiteral::number(discriminant),
        }))
    }

    fn body(&self) -> &Body {
        &self.body
    }

    fn type_name(&self) -> &TypeName {
        match &self.body {
            Body::Inline(inline) => &inline.ty,
            Body::Accessor(accessor) => &accessor.ty,
        }
    }

    fn initial_value(&self) -> String {
        match &self.body {
            Body::Inline(inline) => inline.value.to_string(),
            Body::Accessor(_) => format!("undefined as unknown as {}", self.type_name()),
        }
    }

    fn member(&self, style: MemberStyle) -> String {
        match style {
            MemberStyle::Object => format!("{}: {},", self.name, self.initial_value()),
            MemberStyle::Class => format!(
                "static readonly {}: {} = {};",
                self.name,
                self.type_name(),
                self.initial_value()
            ),
        }
    }

    fn initializer(&self) -> Option<String> {
        let Body::Accessor(accessor) = &self.body else {
            return None;
        };
        Some(match &self.owner {
            Some(owner) => format!(
                "  Object.assign({}, {{ {}: {}() }});\n",
                owner, self.name, accessor.reader
            ),
            None => format!("  {} = {}();\n", self.name, accessor.reader),
        })
    }

    fn owner_name(owner: ConstantOwner, context: &RenderContext<Wasm32>) -> Result<TypeName> {
        let name = context
            .constant_owner_name(owner)
            .ok_or_else(|| Self::unsupported("missing associated constant owner"))?;
        Ok(Name::new(name).type_name())
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl AssociatedConstants {
    pub(super) fn object(owner: ConstantOwner, context: &RenderContext<Wasm32>) -> Result<Self> {
        Self::new(owner, MemberStyle::Object, context)
    }

    pub(super) fn class(owner: ConstantOwner, context: &RenderContext<Wasm32>) -> Result<Self> {
        Self::new(owner, MemberStyle::Class, context)
    }

    fn new(
        owner: ConstantOwner,
        style: MemberStyle,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        let constants = context
            .associated_constants(owner)
            .map(|constant| Constant::from_declaration(constant, context))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            members: constants
                .iter()
                .map(|constant| constant.member(style))
                .collect(),
            functions: constants
                .iter()
                .filter_map(|constant| match &constant.body {
                    Body::Accessor(accessor) => Some(accessor.function.clone()),
                    Body::Inline(_) => None,
                })
                .collect(),
        })
    }
}
