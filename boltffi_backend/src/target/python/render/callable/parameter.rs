use boltffi_binding::{
    CanonicalName, DefaultValue, DirectValueType, DirectVectorElementType, HandlePresence,
    HandleTarget, IncomingParam, IntoRust, Native, ParamDecl, ParamPlan, ParamPlanRender,
    Primitive, Receive, TypeRef, WritePlan, native,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::{EncodedCrossing, Expression as CodecExpression},
        name_style::Name,
        syntax::{Expression, Identifier, Literal, Statement, TypeAnnotation},
    },
};

use super::super::{NameScope, Package, default_value::DefaultExpression, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameters {
    entries: Vec<ParameterStub>,
    first_default: Option<usize>,
    last_required: Option<usize>,
}

impl Parameters {
    pub fn from_declarations(
        parameters: &[ParamDecl<Native, IntoRust>],
        package: &Package,
    ) -> Result<Self> {
        let entries = parameters
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            first_default: entries
                .iter()
                .position(|parameter| parameter.default.is_some()),
            last_required: entries
                .iter()
                .rposition(|parameter| parameter.default.is_none()),
            entries,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParameterStub> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn uses_omission_marker(&self) -> bool {
        self.has_stub_overloads()
            || self.entries.iter().any(|parameter| {
                parameter
                    .default
                    .as_ref()
                    .is_some_and(DefaultExpression::is_deferred)
            })
    }

    pub fn has_stub_overloads(&self) -> bool {
        matches!((self.first_default, self.last_required), (Some(first), Some(last)) if first < last)
    }

    pub fn declaration(&self) -> String {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, parameter)| match &parameter.default {
                Some(DefaultExpression::Immediate(value)) => {
                    format!("{}: {} = {value}", parameter.name, parameter.annotation)
                }
                Some(DefaultExpression::Deferred(_)) => {
                    format!(
                        "{}: {} | _EllipsisType = ...",
                        parameter.name, parameter.annotation
                    )
                }
                None if self.required_after_default(index) => {
                    format!(
                        "{}: {} | _EllipsisType = ...",
                        parameter.name, parameter.annotation
                    )
                }
                None => format!("{}: {}", parameter.name, parameter.annotation),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn stub_declarations(&self) -> Vec<String> {
        std::iter::once(self.stub_declaration(None))
            .chain(
                self.entries
                    .iter()
                    .enumerate()
                    .filter(|(index, parameter)| {
                        parameter.default.is_some()
                            && self.last_required.is_some_and(|required| *index < required)
                    })
                    .map(|(index, _)| self.stub_declaration(Some(index))),
            )
            .collect()
    }

    pub fn initializations(&self) -> impl Iterator<Item = Statement> + '_ {
        let marker = Expression::literal(Literal::ellipsis());
        self.entries
            .iter()
            .enumerate()
            .filter(|(index, _)| self.required_after_default(*index))
            .flat_map({
                let marker = marker.clone();
                move |(_, parameter)| {
                    Statement::if_identical(
                        Expression::identifier(parameter.name.clone()),
                        marker.clone(),
                        Statement::raise_type_error(Literal::string(&format!(
                            "missing required argument '{}'",
                            parameter.name
                        ))),
                    )
                }
            })
            .chain(
                self.entries
                    .iter()
                    .filter_map(move |parameter| {
                        let Some(DefaultExpression::Deferred(value)) = &parameter.default else {
                            return None;
                        };
                        Some(Statement::if_identical(
                            Expression::identifier(parameter.name.clone()),
                            marker.clone(),
                            Statement::assign(parameter.name.clone(), value.clone()),
                        ))
                    })
                    .flatten(),
            )
    }

    pub fn scope(&self, label: impl Into<String>) -> Result<NameScope> {
        let scope = NameScope::new(label);
        let scope = if self.uses_omission_marker() {
            scope.insert("_EllipsisType", "imported omission marker type")?
        } else {
            scope
        };
        scope.insert_all(self.entries.iter().map(ParameterStub::parameter_name))
    }

    fn required_after_default(&self, index: usize) -> bool {
        self.entries[index].default.is_none()
            && self.first_default.is_some_and(|first| index > first)
    }

    fn stub_declaration(&self, keyword_start: Option<usize>) -> String {
        let mut declarations = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let declaration = format!("{}: {}", parameter.name, parameter.annotation);
                let may_omit = keyword_start.is_some_and(|start| index >= start)
                    || self.last_required.is_none_or(|required| index > required);
                match (&parameter.default, may_omit) {
                    (Some(DefaultExpression::Immediate(value)), true) => {
                        format!("{declaration} = {value}")
                    }
                    (Some(DefaultExpression::Deferred(_)), true) => format!("{declaration} = ..."),
                    _ => declaration,
                }
            })
            .collect::<Vec<_>>();
        if let Some(start) = keyword_start {
            declarations.insert(start, "*".to_owned());
        }
        declarations.join(", ")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterStub {
    pub name: Identifier,
    pub annotation: TypeAnnotation,
    pub argument: Expression,
    default: Option<DefaultExpression>,
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
            let IncomingParam::Closure(closure) = parameter.payload() else {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown incoming parameter",
                });
            };
            let annotation = TypeAnnotation::callable_any_object();
            let annotation = match closure.presence() {
                HandlePresence::Nullable => TypeAnnotation::optional(annotation),
                _ => annotation,
            };
            let default = parameter
                .meta()
                .default()
                .map(|value| match value {
                    DefaultValue::Null if closure.presence() == HandlePresence::Nullable => Ok(
                        DefaultExpression::Immediate(Expression::literal(Literal::none())),
                    ),
                    _ => Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "non-null closure default",
                    }),
                })
                .transpose()?;
            return Ok(Self {
                name: name.clone(),
                annotation,
                argument: Expression::identifier(name),
                default,
                uses_sequence_annotation: false,
                uses_callable_annotation: true,
                uses_wire_helpers: false,
            });
        };
        let argument = Self::argument(plan, parameter.name(), package)?;
        let uses_wire_helpers = Self::uses_wire(plan, package)?;
        let annotation = TypeHint::from_parameter(plan, package)?;
        let default = parameter
            .meta()
            .default()
            .map(|value| {
                let ty = plan.value_type().ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "default value for this parameter type",
                })?;
                DefaultExpression::new(&ty, value, package)
            })
            .transpose()?;
        Ok(Self {
            name,
            uses_sequence_annotation: annotation.uses_sequence(),
            uses_callable_annotation: false,
            annotation: annotation.into_annotation(),
            argument,
            default,
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
        _: &TypeRef,
        codec: &WritePlan,
        _: native::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        match (
            self.package.has_native_encoded_crossing(codec.root())?,
            EncodedCrossing::of(codec.root()),
            receive,
        ) {
            (true, _, Receive::ByValue | Receive::ByRef)
            | (false, EncodedCrossing::Utf8Text, Receive::ByValue | Receive::ByRef) => {
                Ok(Expression::identifier(self.name.clone()))
            }
            _ => CodecExpression::write_argument(codec, self.package)
                .map(CodecExpression::into_expression),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        Ok(Expression::identifier(self.name.clone()))
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
        receive: Receive,
    ) -> Self::Output {
        match (
            shape,
            self.package.has_native_encoded_crossing(codec.root())?,
            EncodedCrossing::of(codec.root()),
            receive,
        ) {
            (
                native::BufferShape::Slice,
                false,
                EncodedCrossing::Utf8Text,
                Receive::ByValue | Receive::ByRef,
            )
            | (native::BufferShape::Slice, true, _, Receive::ByValue | Receive::ByRef) => Ok(false),
            (native::BufferShape::Slice, _, _, _) => {
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
