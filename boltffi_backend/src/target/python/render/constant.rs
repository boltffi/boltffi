use boltffi_binding::{
    ConstantDecl, ConstantValueDecl, CustomTypeId, DefaultValue, FieldKey, Native, TypeRef,
};

use crate::{
    core::{
        Error, Result,
        default_value::{Field as RepresentationField, Representation},
    },
    target::python::{
        name_style::Name,
        syntax::{CallExpression, Expression, Identifier, Literal, TypeAnnotation},
    },
};

use super::{Documentation, Package, callable::ReturnStub, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstantStub {
    pub documentation: Documentation,
    pub owner: Option<Identifier>,
    pub python_name: Identifier,
    pub annotation: TypeAnnotation,
    pub expression: Expression,
    uses_wire_helpers: bool,
}

impl ConstantStub {
    pub fn from_declaration(constant: &ConstantDecl<Native>, package: &Package) -> Result<Self> {
        let owner_name = constant
            .owner()
            .map(|owner| package.constant_owner_canonical_name(owner))
            .transpose()?;
        let owner = owner_name
            .map(|name| Identifier::parse(Name::new(name).class()))
            .transpose()?;
        let python_name = owner_name.map_or_else(
            || Name::new(constant.name()).function(),
            |_| Name::new(constant.name()).constant(),
        )?;
        let documentation = Documentation::new(constant.meta().doc());
        match constant.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                Self::from_inline(documentation, owner, python_name, ty, value, package)
            }
            ConstantValueDecl::Accessor { callable, .. } => {
                let returned = ReturnStub::from_plan(callable.returns().plan(), package)?;
                let native_name = match owner_name {
                    Some(owner) => Name::associated_constant(owner, constant.name())?,
                    None => Name::new(constant.name()).function()?,
                };
                let native_call = Expression::call(CallExpression::new(Expression::attribute(
                    Expression::identifier(Identifier::parse("_native")?),
                    native_name,
                )));
                let expression = returned.expression(native_call)?;
                let uses_wire_helpers = returned.uses_wire_helpers();
                Ok(Self {
                    documentation,
                    owner,
                    python_name,
                    annotation: returned.into_annotation(),
                    expression,
                    uses_wire_helpers,
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown constant value package",
            }),
        }
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.python_name.to_string(),
            format!("constant `{}`", self.python_name),
        )
    }

    pub fn member_name(&self) -> (String, String) {
        (
            self.python_name.to_string(),
            format!("associated constant `{}`", self.python_name),
        )
    }

    fn from_inline(
        documentation: Documentation,
        owner: Option<Identifier>,
        python_name: Identifier,
        ty: &TypeRef,
        value: &DefaultValue,
        package: &Package,
    ) -> Result<Self> {
        Ok(Self {
            documentation,
            owner,
            python_name,
            annotation: TypeHint::from_type_ref(ty, package)?.into_annotation(),
            expression: DefaultExpression::new(ty, value, package)?.into_expression(),
            uses_wire_helpers: false,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultExpression {
    expression: Expression,
}

impl DefaultExpression {
    pub fn new(ty: &TypeRef, value: &DefaultValue, package: &Package) -> Result<Self> {
        if let TypeRef::Optional(inner) = ty {
            return match value {
                DefaultValue::Null => Ok(Self {
                    expression: Expression::literal(Literal::none()),
                }),
                _ => Self::new(inner, value, package),
            };
        }
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, package);
        }
        Ok(Self {
            expression: match value {
                DefaultValue::Bool(value) => Expression::literal(Literal::bool(*value)),
                DefaultValue::Integer(value) => Expression::literal(Literal::integer(value.get())),
                DefaultValue::Float(value) => Literal::float(value.to_f64()),
                DefaultValue::String(value) => Expression::literal(Literal::string(value)),
                DefaultValue::EnumVariant {
                    enum_name,
                    variant_name,
                } => package.enum_variant_expression(enum_name, variant_name)?,
                DefaultValue::Null => Expression::literal(Literal::none()),
                _ => {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "unknown constant literal",
                    });
                }
            },
        })
    }

    fn custom(custom_type: CustomTypeId, value: &DefaultValue, package: &Package) -> Result<Self> {
        match Representation::resolve(custom_type, package.context)? {
            Representation::Transparent(representation) => {
                Self::new(representation, value, package)
            }
            Representation::Record(record) => {
                let value = match record.field() {
                    RepresentationField::Direct(field) => {
                        Self::new(&TypeRef::Primitive(field.ty().primitive()), value, package)?
                    }
                    RepresentationField::Encoded(field) => Self::new(field.ty(), value, package)?,
                };
                let field = match record.field().key() {
                    FieldKey::Named(name) => Name::new(name).function()?,
                    FieldKey::Position(position) => Name::position_field(*position)?,
                    _ => {
                        return Err(Error::UnsupportedTarget {
                            target: "python",
                            shape: "custom type default field name",
                        });
                    }
                };
                Ok(Self {
                    expression: Expression::call(
                        CallExpression::new(Expression::identifier(Identifier::parse(
                            Name::new(record.name()).class(),
                        )?))
                        .keyword(field, value.into_expression()),
                    ),
                })
            }
        }
    }

    pub fn into_expression(self) -> Expression {
        self.expression
    }
}
