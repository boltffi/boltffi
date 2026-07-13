use boltffi_binding::{
    CanonicalName, DirectValueType, DirectVectorElementType, HandlePresence, HandleTarget,
    IncomingParam, IntoRust, Native, ParamDecl, ParamPlan, ParamPlanRender, Primitive, Receive,
    TypeRef, WritePlan, native,
};

use crate::{
    core::Result,
    target::python::{
        codec::Expression as CodecExpression,
        name_style::Name,
        syntax::{CallExpression, Expression, Identifier, Literal, TypeAnnotation},
    },
};

use super::super::{NameScope, Package, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterStub {
    pub name: Identifier,
    pub annotation: TypeAnnotation,
    pub argument: Expression,
    uses_sequence_annotation: bool,
    uses_callable_annotation: bool,
    uses_wire_helpers: bool,
}

impl ParameterStub {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        package: &Package,
    ) -> Result<Self> {
        let name = Name::new(parameter.name()).function()?;
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Ok(Self {
                name: name.clone(),
                annotation: TypeAnnotation::callable_any_object(),
                argument: Expression::identifier(name),
                uses_sequence_annotation: false,
                uses_callable_annotation: true,
                uses_wire_helpers: false,
            });
        };
        let argument = Self::argument(plan, parameter.name(), package)?;
        let uses_wire_helpers = Self::uses_wire(plan, package)?;
        let annotation = TypeHint::from_parameter(plan, package)?;
        Ok(Self {
            name,
            uses_sequence_annotation: annotation.uses_sequence(),
            uses_callable_annotation: false,
            annotation: annotation.into_annotation(),
            argument,
            uses_wire_helpers,
        })
    }
}

impl ParameterStub {
    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn uses_sequence_annotation(&self) -> bool {
        self.uses_sequence_annotation
    }

    pub fn uses_callable_annotation(&self) -> bool {
        self.uses_callable_annotation
    }

    pub fn parameter_name(&self) -> (String, String) {
        (self.name.to_string(), format!("parameter `{}`", self.name))
    }

    pub fn scope(label: impl Into<String>, parameters: &[Self]) -> Result<NameScope> {
        NameScope::new(label).insert_all(parameters.iter().map(Self::parameter_name))
    }

    fn argument(
        plan: &ParamPlan<Native, IntoRust>,
        name: &CanonicalName,
        package: &Package,
    ) -> Result<Expression> {
        plan.render_with(&mut StubArgument {
            name: Name::new(name).function()?,
            package,
        })
    }

    fn uses_wire(plan: &ParamPlan<Native, IntoRust>, package: &Package) -> Result<bool> {
        plan.render_with(&mut WireHelperUse { package })
    }
}

struct StubArgument<'package> {
    name: Identifier,
    package: &'package Package<'package>,
}

impl<'plan, 'package> ParamPlanRender<'plan, Native, IntoRust> for StubArgument<'package> {
    type Output = Result<Expression>;

    fn direct(&mut self, _: &DirectValueType, _: Receive) -> Self::Output {
        Ok(Expression::identifier(self.name.clone()))
    }

    fn encoded(
        &mut self,
        ty: &TypeRef,
        codec: &WritePlan,
        _: native::BufferShape,
        transport: boltffi_binding::EncodedParamTransport,
        _: Receive,
    ) -> Self::Output {
        match transport {
            boltffi_binding::EncodedParamTransport::Wire => {
                CodecExpression::write_argument(codec, self.package)
                    .map(CodecExpression::into_expression)
            }
            boltffi_binding::EncodedParamTransport::BorrowedSlice => match ty {
                TypeRef::Bytes => Ok(Expression::identifier(self.name.clone())),
                TypeRef::String => Ok(Expression::call(
                    CallExpression::new(Expression::attribute(
                        Expression::identifier(self.name.clone()),
                        Identifier::parse("encode")?,
                    ))
                    .positional(Expression::literal(Literal::string("utf-8"))),
                )),
                _ => Err(crate::core::Error::UnsupportedTarget {
                    target: "python",
                    shape: "borrowed slice parameter type",
                }),
            },
            _ => Err(crate::core::Error::UnsupportedTarget {
                target: "python",
                shape: "unknown encoded parameter transport",
            }),
        }
    }

    fn handle(
        &mut self,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        match (target, presence) {
            (HandleTarget::Class(_), HandlePresence::Required) => Ok(Expression::attribute(
                Expression::identifier(self.name.clone()),
                Identifier::parse("_handle")?,
            )),
            (HandleTarget::Class(_), HandlePresence::Nullable) => Ok(Expression::conditional(
                Expression::literal(Literal::integer(0)),
                Expression::is_none(Expression::identifier(self.name.clone())),
                Expression::attribute(
                    Expression::identifier(self.name.clone()),
                    Identifier::parse("_handle")?,
                ),
            )),
            _ => Ok(Expression::identifier(self.name.clone())),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(Expression::identifier(self.name.clone()))
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType, _: Receive) -> Self::Output {
        Ok(Expression::identifier(self.name.clone()))
    }
}

struct WireHelperUse<'package> {
    package: &'package Package<'package>,
}

impl<'plan, 'package> ParamPlanRender<'plan, Native, IntoRust> for WireHelperUse<'package> {
    type Output = Result<bool>;

    fn direct(&mut self, _: &DirectValueType, _: Receive) -> Self::Output {
        Ok(false)
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        codec: &WritePlan,
        shape: native::BufferShape,
        transport: boltffi_binding::EncodedParamTransport,
        _: Receive,
    ) -> Self::Output {
        match (shape, transport) {
            (native::BufferShape::Slice, boltffi_binding::EncodedParamTransport::Wire) => {
                CodecExpression::write_argument(codec, self.package).map(|_| true)
            }
            _ => Ok(false),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        Ok(false)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(false)
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType, _: Receive) -> Self::Output {
        Ok(false)
    }
}
