use boltffi_binding::{
    ClosureReturn, CodecNode, DirectValueType, DirectVectorElementType, ErrorChannel,
    ErrorPlacement, ExportedCallable, HandlePresence, HandleTarget, IncomingParam, IntoRust,
    Native, OutOfRust, ParamDecl, ParamPlanRender, Primitive, ReadPlan, Receive, RecordId,
    ReturnPlan, ReturnPlanRender, ReturnValueSlot, TypeRef, WritePlan, native,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::Expression as CodecExpression,
        syntax::{CallExpression, Expression, Identifier, Statement, TypeAnnotation},
    },
};

use super::super::{Package, type_hint::TypeHint};

pub struct ReturnStub {
    annotation: TypeAnnotation,
    value: ReturnedValue,
}

impl ReturnStub {
    pub fn native(annotation: TypeAnnotation) -> Self {
        Self {
            annotation,
            value: ReturnedValue::Native,
        }
    }

    pub fn from_callable(callable: &ExportedCallable<Native>, package: &Package) -> Result<Self> {
        if let Some(returned) = Self::from_mutable_parameter(callable, package)? {
            return Ok(returned);
        }
        match callable.error().channel() {
            ErrorChannel::None => Self::from_plan(callable.returns().plan(), package),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_success_plan(callable.returns().plan(), package),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ..
            } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error buffer shape",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible callable stub",
            }),
        }
    }

    pub fn from_plan(plan: &ReturnPlan<Native, OutOfRust>, package: &Package) -> Result<Self> {
        Ok(Self {
            annotation: TypeHint::from_return(plan, package)?.into_annotation(),
            value: ReturnedValue::from_plan(plan, package)?,
        })
    }
}

impl ReturnStub {
    pub fn expression(&self, native_call: Expression) -> Result<Expression> {
        self.value.expression(native_call)
    }

    pub fn into_annotation(self) -> TypeAnnotation {
        self.annotation
    }

    pub fn returned_value(&self) -> &ReturnedValue {
        &self.value
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.value.uses_wire_helpers()
    }

    fn from_success_plan(plan: &ReturnPlan<Native, OutOfRust>, package: &Package) -> Result<Self> {
        Ok(Self {
            annotation: TypeHint::from_return(plan, package)?.into_annotation(),
            value: ReturnedValue::from_success_plan(plan, package)?,
        })
    }

    fn from_mutable_parameter(
        callable: &ExportedCallable<Native>,
        package: &Package,
    ) -> Result<Option<Self>> {
        if !matches!(callable.error().channel(), ErrorChannel::None)
            || !matches!(callable.returns().plan(), ReturnPlan::Void)
        {
            return Ok(None);
        }
        callable
            .params()
            .iter()
            .map(|parameter| MutableParameterReturn { package }.render(parameter))
            .collect::<Result<Vec<_>>>()
            .map(|returned| returned.into_iter().flatten().next())
    }
}

pub enum ReturnedValue {
    Void,
    Native,
    ClassHandle(Identifier),
    Wire(Expression),
}

impl ReturnedValue {
    pub fn class_handle(class_name: Identifier) -> Self {
        Self::ClassHandle(class_name)
    }

    pub fn from_plan(plan: &ReturnPlan<Native, OutOfRust>, package: &Package) -> Result<Self> {
        plan.render_with(&mut ReturnedValueRender::new(
            package,
            ReturnDelivery::Callable,
        ))
    }

    pub fn from_success_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package,
    ) -> Result<Self> {
        plan.render_with(&mut ReturnedValueRender::new(
            package,
            ReturnDelivery::FallibleSuccess,
        ))
    }

    pub fn statement(&self, native_call: Expression) -> Result<Statement> {
        match self {
            Self::Void => Ok(Statement::expression(native_call)),
            Self::Native | Self::ClassHandle(_) | Self::Wire(_) => {
                self.expression(native_call).map(Statement::return_value)
            }
        }
    }

    pub fn expression(&self, native_call: Expression) -> Result<Expression> {
        match self {
            Self::Void | Self::Native => Ok(native_call),
            Self::ClassHandle(class_name) => Ok(Expression::call(
                CallExpression::new(Expression::attribute(
                    Expression::identifier(class_name.clone()),
                    Identifier::parse("_from_handle")?,
                ))
                .positional(native_call),
            )),
            Self::Wire(decode) => Ok(Expression::call(
                CallExpression::new(Expression::identifier(Identifier::parse(
                    "_boltffi_read_wire",
                )?))
                .positional(native_call)
                .positional(Expression::lambda(
                    Identifier::parse("reader")?,
                    decode.clone(),
                )),
            )),
        }
    }

    pub fn uses_wire_helpers(&self) -> bool {
        matches!(self, Self::Wire(_))
    }

    pub fn awaited_statement(&self, wait_call: Expression) -> Result<Vec<Statement>> {
        let value = Identifier::parse("__boltffi_value")?;
        let awaited = Expression::await_value(wait_call);
        match self {
            Self::Void => Ok(vec![Statement::expression(awaited)]),
            Self::Native => Ok(vec![Statement::return_value(awaited)]),
            Self::ClassHandle(_) | Self::Wire(_) => Ok(vec![
                Statement::assign(value.clone(), awaited),
                Statement::return_value(self.expression(Expression::identifier(value))?),
            ]),
        }
    }

    fn from_encoded_plan(codec: &ReadPlan, package: &Package) -> Result<Self> {
        CodecExpression::read_return(codec, package)
            .map(|decode| Self::Wire(decode.into_expression()))
    }

    fn from_encoded_codec(codec: &CodecNode, package: &Package) -> Result<Self> {
        CodecExpression::read_codec(codec, package)
            .map(|decode| Self::Wire(decode.into_expression()))
    }
}

struct MutableParameterReturn<'package> {
    package: &'package Package<'package>,
}

impl<'package> MutableParameterReturn<'package> {
    fn render(&self, parameter: &ParamDecl<Native, IntoRust>) -> Result<Option<ReturnStub>> {
        match parameter.payload() {
            IncomingParam::Value(plan) => plan.render_with(&mut MutableParameterReturnRender {
                package: self.package,
            }),
            IncomingParam::Closure(_) => Ok(None),
        }
    }
}

struct MutableParameterReturnRender<'package> {
    package: &'package Package<'package>,
}

impl<'plan, 'package> ParamPlanRender<'plan, Native, IntoRust>
    for MutableParameterReturnRender<'package>
{
    type Output = Result<Option<ReturnStub>>;

    fn direct(&mut self, _: &DirectValueType, _: Receive) -> Self::Output {
        Ok(None)
    }

    fn encoded(
        &mut self,
        ty: &TypeRef,
        codec: &WritePlan,
        shape: native::BufferShape,
        _: boltffi_binding::EncodedParamTransport,
        receive: Receive,
    ) -> Self::Output {
        if receive != Receive::ByMutRef {
            return Ok(None);
        }
        match shape {
            native::BufferShape::Slice => Ok(Some(ReturnStub {
                annotation: TypeHint::from_type_ref(ty, self.package)?.into_annotation(),
                value: ReturnedValue::from_encoded_codec(codec.root(), self.package)?,
            })),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "mutable encoded parameter",
            }),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        Ok(None)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(None)
    }

    fn direct_vector(
        &mut self,
        element: &DirectVectorElementType,
        receive: Receive,
    ) -> Self::Output {
        match receive {
            Receive::ByMutRef => Ok(Some(ReturnStub {
                annotation: TypeHint::from_direct_vector_parameter(element, self.package)?
                    .into_annotation(),
                value: ReturnedValue::Native,
            })),
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ReturnDelivery {
    Callable,
    FallibleSuccess,
}

impl ReturnDelivery {
    fn slot(self, slot: ReturnValueSlot) -> Result<()> {
        match slot {
            _ if self == Self::Callable => Ok(()),
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnsupportedTarget {
                target: "python",
                shape: self.unsupported_shape(),
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown return stub",
            }),
        }
    }

    fn native(self) -> Result<ReturnedValue> {
        Ok(ReturnedValue::Native)
    }

    fn unsupported_shape(self) -> &'static str {
        match self {
            Self::Callable => "unknown return stub",
            Self::FallibleSuccess => "fallible success return",
        }
    }
}

struct ReturnedValueRender<'package> {
    package: &'package Package<'package>,
    delivery: ReturnDelivery,
}

impl<'package> ReturnedValueRender<'package> {
    fn new(package: &'package Package<'package>, delivery: ReturnDelivery) -> Self {
        Self { package, delivery }
    }
}

impl<'plan, 'package> ReturnPlanRender<'plan, Native, OutOfRust> for ReturnedValueRender<'package> {
    type Output = Result<ReturnedValue>;

    fn void(&mut self) -> Self::Output {
        Ok(ReturnedValue::Void)
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &DirectValueType) -> Self::Output {
        self.delivery
            .slot(slot)
            .and_then(|()| self.delivery.native())
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &TypeRef,
        codec: &ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        self.delivery.slot(slot)?;
        match shape {
            native::BufferShape::Buffer => ReturnedValue::from_encoded_plan(codec, self.package),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: self.delivery.unsupported_shape(),
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        self.delivery.slot(slot)?;
        match (target, presence) {
            (HandleTarget::Class(class_id), HandlePresence::Required) => Ok(
                ReturnedValue::class_handle(self.package.class_name(class_id)?),
            ),
            _ => self.delivery.native(),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        self.delivery.native()
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType) -> Self::Output {
        self.delivery.native()
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: self.delivery.unsupported_shape(),
        })
    }

    fn native_opaque_record(&mut self, _record: RecordId) -> Self::Output {
        // The C converter (boltffi_python_box_opaque_*) already calls _from_handle
        // and returns a fully-constructed Python object. Package.py must NOT call
        // _from_handle again; returning Native gives a direct passthrough.
        Ok(ReturnedValue::Native)
    }
}
