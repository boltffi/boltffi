use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClosureParameter, ClosureReturn, DirectValueType, DirectVectorElementType, ErrorChannel,
    ErrorPlacement, HandlePresence, HandleTarget, IntoRust, Native, OutOfRust, OutgoingParam,
    ParamDecl, ParamPlanRender, Primitive, ReadPlan, ReturnPlan, ReturnPlanRender, ReturnValueSlot,
    TypeRef, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Expression, Identifier, Literal, TypeFragment},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Error, RenderContext, Result},
    target::python::{
        codec::{BorrowedPayload, Marshaling, OwnedPayload},
        cpython::render::{direct, direct_vector, primitive},
        name_style::Name,
    },
};

use super::fallible::FallibleReturn;

#[derive(AskamaTemplate)]
#[template(path = "target/python/closure.c", escape = "none")]
struct Template {
    invoke: Identifier,
    release: Identifier,
    parser: Identifier,
    call_output_declaration: c::Statement,
    context_output_declaration: c::Statement,
    release_output_declaration: c::Statement,
    copy_buffer_storage: Identifier,
    params: Vec<Argument>,
    returns: ReturnValue,
    fallible_return: Option<FallibleReturn>,
    wire_payload: bool,
}

pub struct Parameter {
    call_declaration: c::Statement,
    call: Identifier,
    context_declaration: c::Statement,
    context: Identifier,
    release_declaration: c::Statement,
    release: Identifier,
    parser: Identifier,
    release_needed: Identifier,
    source: String,
    primitives: Vec<primitive::Runtime>,
    wire_primitives: Vec<primitive::Runtime>,
    direct_vectors: Vec<direct_vector::Element>,
    string_argument: bool,
    bytes_argument: bool,
    raw_wire_argument: bool,
}

impl Parameter {
    pub fn new(
        owner: &str,
        index: usize,
        name: Identifier,
        parameter: &ClosureParameter<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let signature = Signature::new(parameter, c_parameters, bridge, context)?;
        let prefix = Identifier::escape(format!("{owner}_{index}_{name}"))?;
        let invoke = Identifier::parse(format!("boltffi_python_closure_{prefix}_invoke"))?;
        let release = Identifier::parse(format!("boltffi_python_closure_{prefix}_release"))?;
        let parser = Identifier::parse(format!("boltffi_python_parse_closure_{prefix}"))?;
        let call = Identifier::parse(c_parameters[0].name())?;
        let context_name = Identifier::parse(c_parameters[1].name())?;
        let release_name = Identifier::parse(c_parameters[2].name())?;
        let release_needed = Identifier::escape(format!("{name}_release_needed"))?;
        let copy_buffer_storage = Self::copy_buffer_storage(bridge)?;
        let source = Template {
            invoke,
            release,
            parser: parser.clone(),
            call_output_declaration: OutputParameter::new(c_parameters[0].ty(), "out_call")
                .declaration()?,
            context_output_declaration: OutputParameter::new(c_parameters[1].ty(), "out_context")
                .declaration()?,
            release_output_declaration: OutputParameter::new(c_parameters[2].ty(), "out_release")
                .declaration()?,
            copy_buffer_storage,
            wire_payload: signature.wire_payload(),
            params: signature.params.clone(),
            returns: signature.returns.clone(),
            fallible_return: signature.fallible_return.clone(),
        }
        .render()?;
        Ok(Self {
            call_declaration: TypeFragment::declaration(c_parameters[0].ty(), call.as_str())?,
            call,
            context_declaration: TypeFragment::declaration(
                c_parameters[1].ty(),
                context_name.as_str(),
            )?,
            context: context_name,
            release_declaration: TypeFragment::declaration(
                c_parameters[2].ty(),
                release_name.as_str(),
            )?,
            release: release_name,
            parser,
            release_needed,
            source,
            primitives: signature.primitives(),
            wire_primitives: signature.wire_primitives(),
            direct_vectors: signature.direct_vectors(),
            string_argument: signature.has_string_argument(),
            bytes_argument: signature.has_bytes_argument(),
            raw_wire_argument: signature.has_raw_wire_argument(),
        })
    }

    pub fn c_arity() -> usize {
        3
    }

    pub fn call_args(&self) -> [Identifier; 3] {
        [
            self.call.clone(),
            self.context.clone(),
            self.release.clone(),
        ]
    }

    pub fn call_declaration(&self) -> &c::Statement {
        &self.call_declaration
    }

    pub fn context_declaration(&self) -> &c::Statement {
        &self.context_declaration
    }

    pub fn release_declaration(&self) -> &c::Statement {
        &self.release_declaration
    }

    pub fn declaration(&self) -> &str {
        &self.source
    }

    pub fn parser(&self) -> &Identifier {
        &self.parser
    }

    pub fn call(&self) -> &Identifier {
        &self.call
    }

    pub fn context(&self) -> &Identifier {
        &self.context
    }

    pub fn release(&self) -> &Identifier {
        &self.release
    }

    pub fn release_needed(&self) -> &Identifier {
        &self.release_needed
    }

    pub fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.primitives.iter().copied()
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.wire_primitives.iter().copied()
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.direct_vectors.iter().cloned()
    }

    pub fn has_string_argument(&self) -> bool {
        self.string_argument
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.bytes_argument
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.raw_wire_argument
    }

    fn copy_buffer_storage(bridge: &PythonCExtBridgeContract) -> Result<Identifier> {
        Ok(bridge.buffer_from_bytes()?.storage_name().clone())
    }
}

struct OutputParameter {
    ty: c::Type,
    name: &'static str,
}

impl OutputParameter {
    fn new(ty: &c::Type, name: &'static str) -> Self {
        Self {
            ty: ty.clone(),
            name,
        }
    }

    fn declaration(&self) -> Result<c::Statement> {
        match &self.ty {
            c::Type::FunctionPointer { returns, params } => {
                TypeFragment::function_pointer_declaration(
                    format!("*{}", self.name).as_str(),
                    returns,
                    params.iter(),
                )
            }
            _ => Ok(c::Statement::new(format!(
                "{} *{}",
                TypeFragment::anonymous(&self.ty)?,
                self.name
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Signature {
    params: Vec<Argument>,
    returns: ReturnValue,
    fallible_return: Option<FallibleReturn>,
}

impl Signature {
    fn new(
        parameter: &ClosureParameter<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [call, context_param, release_param, ..] = c_parameters else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure parameter ABI",
            });
        };
        Self::validate_context(context_param.ty())?;
        Self::validate_release(release_param.ty())?;
        let c::Type::FunctionPointer {
            returns,
            params: call_params,
        } = call.ty()
        else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure call ABI",
            });
        };
        let invoke = parameter.invoke();
        let arity = invoke
            .params()
            .iter()
            .map(Argument::arity)
            .collect::<Result<Vec<_>>>()?;
        let return_out_count = Self::return_out_count(invoke.returns().plan())?;
        let value_count = arity.iter().sum::<usize>();
        let value_start = 1;
        let value_end = value_start + value_count;
        if call_params.len() != value_end + return_out_count {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure invoke ABI",
            });
        }
        let param_types = arity
            .iter()
            .scan(value_start, |offset, count| {
                let start = *offset;
                *offset += *count;
                Some(&call_params[start..*offset])
            })
            .collect::<Vec<_>>();
        let params = invoke
            .params()
            .iter()
            .zip(param_types)
            .map(|(parameter, c_types)| Argument::new(parameter, c_types, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let return_out = (return_out_count == 1).then(|| &call_params[value_end]);
        let fallible_return = match invoke.error().channel() {
            ErrorChannel::None => None,
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ..
            } => Some(FallibleReturn::new(
                invoke.returns().plan(),
                invoke.error(),
                return_out,
                bridge,
                context,
            )?),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "closure error channel",
                });
            }
        };
        let returns = match &fallible_return {
            Some(_) => ReturnValue::fallible_error(returns)?,
            None => ReturnValue::new(invoke.returns().plan(), returns, bridge, context)?,
        };
        Ok(Self {
            params,
            returns,
            fallible_return,
        })
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        self.params
            .iter()
            .filter_map(Argument::primitive)
            .chain(self.returns.primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::primitives),
            )
            .collect()
    }

    fn wire_primitives(&self) -> Vec<primitive::Runtime> {
        self.params
            .iter()
            .filter_map(Argument::wire_primitive)
            .chain(self.returns.wire_primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::wire_primitives),
            )
            .collect()
    }

    fn direct_vectors(&self) -> Vec<direct_vector::Element> {
        self.params
            .iter()
            .filter_map(Argument::direct_vector_element)
            .chain(self.returns.direct_vector())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::direct_vectors),
            )
            .collect()
    }

    fn wire_payload(&self) -> bool {
        self.returns.wire
            || self
                .fallible_return
                .as_ref()
                .is_some_and(FallibleReturn::uses_wire_payload)
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(Argument::has_string)
            || self.returns.has_string()
            || self.fallible_return.iter().any(FallibleReturn::has_string)
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(Argument::has_bytes)
            || self.returns.has_bytes()
            || self.fallible_return.iter().any(FallibleReturn::has_bytes)
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(Argument::has_raw_wire)
            || self.returns.has_raw_wire()
            || self
                .fallible_return
                .iter()
                .any(FallibleReturn::has_raw_wire)
    }

    fn validate_context(ty: &c::Type) -> Result<()> {
        match ty {
            c::Type::MutPointer(inner) if matches!(inner.as_ref(), c::Type::Void) => Ok(()),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure context ABI",
            }),
        }
    }

    fn validate_release(ty: &c::Type) -> Result<()> {
        match ty {
            c::Type::FunctionPointer { returns, params }
                if matches!(returns.as_ref(), c::Type::Void)
                    && matches!(
                        params.as_slice(),
                        [c::Type::MutPointer(inner)] if matches!(inner.as_ref(), c::Type::Void)
                    ) =>
            {
                Ok(())
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure release ABI",
            }),
        }
    }

    fn return_out_count(plan: &ReturnPlan<Native, IntoRust>) -> Result<usize> {
        plan.render_with(&mut ClosureReturnOutCount)
    }
}

struct ClosureReturnOutCount;

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ClosureReturnOutCount {
    type Output = Result<usize>;

    fn void(&mut self) -> Self::Output {
        Ok(0)
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {
        Self::slot_count(slot)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan WritePlan,
        _: native::BufferShape,
    ) -> Self::Output {
        Self::slot_count(slot)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        Self::slot_count(slot)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(0)
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(0)
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "closure return from closure parameter",
        })
    }
}

impl ClosureReturnOutCount {
    fn slot_count(slot: ReturnValueSlot) -> Result<usize> {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(0),
            ReturnValueSlot::OutPointer => Ok(1),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown closure return plan",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Argument {
    declarations: Vec<c::Statement>,
    object: Identifier,
    expression: c::Expression,
    marshaling: Marshaling,
}

impl Argument {
    fn arity(parameter: &ParamDecl<Native, OutOfRust>) -> Result<usize> {
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut ClosureArgumentArity),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown closure argument",
            }),
        }
    }

    fn new(
        parameter: &ParamDecl<Native, OutOfRust>,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Identifier::escape(Name::new(parameter.name()).function_text()?)?;
        let object = Identifier::parse(format!("{name}_object"))?;
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut ClosureArgumentValue {
                name,
                object,
                c_types: c_types.to_vec(),
                bridge,
                context,
            }),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported closure argument",
            }),
        }
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.marshaling.primitive()
    }

    fn wire_primitive(&self) -> Option<primitive::Runtime> {
        self.marshaling.wire_primitive()
    }

    fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        self.marshaling.direct_vector_element()
    }

    fn has_string(&self) -> bool {
        self.marshaling.has_string()
    }

    fn has_bytes(&self) -> bool {
        self.marshaling.has_bytes()
    }

    fn has_raw_wire(&self) -> bool {
        self.marshaling.has_raw_wire()
    }

    fn direct(
        name: Identifier,
        object: Identifier,
        ty: &DirectValueType,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [c_type] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure direct argument ABI",
            });
        };
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        Ok(Self {
            declarations: vec![TypeFragment::declaration(c_type, name.as_str())?],
            object,
            expression: direct.box_expression(name),
            marshaling: Marshaling::direct(direct.primitive()),
        })
    }

    fn encoded(
        name: Identifier,
        object: Identifier,
        codec: &ReadPlan,
        c_types: &[c::Type],
    ) -> Result<Self> {
        let [pointer, length] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure encoded argument ABI",
            });
        };
        let pointer_name = Identifier::parse(format!("{name}_ptr"))?;
        let length_name = Identifier::parse(format!("{name}_len"))?;
        let payload = BorrowedPayload::read(codec, pointer_name.clone(), length_name.clone())?;
        let marshaling = payload.marshaling();
        let expression = payload.expression();
        Ok(Self {
            declarations: vec![
                TypeFragment::declaration(pointer, pointer_name.as_str())?,
                TypeFragment::declaration(length, length_name.as_str())?,
            ],
            object,
            expression,
            marshaling,
        })
    }

    fn direct_vector(
        name: Identifier,
        object: Identifier,
        element: &DirectVectorElementType,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [pointer, length] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure vector argument ABI",
            });
        };
        let pointer_name = Identifier::parse(format!("{name}_ptr"))?;
        let length_name = Identifier::parse(format!("{name}_len"))?;
        let element = direct_vector::Element::from_element(element, bridge, context)?;
        Ok(Self {
            declarations: vec![
                TypeFragment::declaration(pointer, pointer_name.as_str())?,
                TypeFragment::declaration(length, length_name.as_str())?,
            ],
            object,
            expression: c::Expression::call(
                element.vector_boxer().clone(),
                c::ArgumentList::from_iter([
                    c::Expression::identifier(pointer_name),
                    c::Expression::identifier(length_name),
                ]),
            ),
            marshaling: Marshaling::direct_vector(element),
        })
    }
}

struct ClosureArgumentArity;

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ClosureArgumentArity {
    type Output = Result<usize>;

    fn direct(&mut self, _: &DirectValueType, _: ()) -> Self::Output {
        Ok(1)
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        _: &ReadPlan,
        shape: native::BufferShape,
        _: (),
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => Ok(2),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported closure argument",
            }),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType, _: ()) -> Self::Output {
        Ok(2)
    }
}

struct ClosureArgumentValue<'render> {
    name: Identifier,
    object: Identifier,
    c_types: Vec<c::Type>,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ParamPlanRender<'plan, Native, OutOfRust> for ClosureArgumentValue<'render> {
    type Output = Result<Argument>;

    fn direct(&mut self, ty: &DirectValueType, _: ()) -> Self::Output {
        Argument::direct(
            self.name.clone(),
            self.object.clone(),
            ty,
            &self.c_types,
            self.bridge,
            self.context,
        )
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        codec: &ReadPlan,
        shape: native::BufferShape,
        _: (),
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => {
                Argument::encoded(self.name.clone(), self.object.clone(), codec, &self.c_types)
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported closure argument",
            }),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType, _: ()) -> Self::Output {
        Argument::direct_vector(
            self.name.clone(),
            self.object.clone(),
            element,
            &self.c_types,
            self.bridge,
            self.context,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReturnValue {
    c_type: TypeFragment,
    parser: Option<Identifier>,
    default_value: Expression,
    value: Option<Identifier>,
    marshaling: Marshaling,
    wire: bool,
    void: bool,
}

impl ReturnValue {
    fn fallible_error(c_type: &c::Type) -> Result<Self> {
        Ok(Self {
            c_type: TypeFragment::anonymous(c_type)?,
            parser: None,
            default_value: Expression::literal(Literal::compound_zero()),
            value: Some(Identifier::parse("return_value")?),
            marshaling: Marshaling::none(),
            wire: true,
            void: false,
        })
    }

    fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        c_type: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut ClosureReturnValue {
            c_type: c_type.clone(),
            bridge,
            context,
        })
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.marshaling.primitive()
    }

    fn wire_primitive(&self) -> Option<primitive::Runtime> {
        self.marshaling.wire_primitive()
    }

    fn direct_vector(&self) -> Option<direct_vector::Element> {
        self.marshaling.direct_vector_element()
    }

    fn has_string(&self) -> bool {
        self.marshaling.has_string()
    }

    fn has_bytes(&self) -> bool {
        self.marshaling.has_bytes()
    }

    fn has_raw_wire(&self) -> bool {
        self.marshaling.has_raw_wire()
    }

    fn has_value(&self) -> bool {
        !self.void
    }

    fn parser(&self) -> &Identifier {
        self.parser.as_ref().expect("return parser")
    }

    fn value(&self) -> &Identifier {
        self.value.as_ref().expect("return value")
    }

    fn encoded(c_type: &c::Type, codec: &WritePlan) -> Result<Self> {
        let encoded = OwnedPayload::write(codec)?;
        Self::wire(c_type, encoded.parser().clone(), encoded.marshaling())
    }

    fn wire(c_type: &c::Type, parser: Identifier, marshaling: Marshaling) -> Result<Self> {
        Ok(Self {
            c_type: TypeFragment::anonymous(c_type)?,
            parser: Some(parser),
            default_value: Expression::literal(Literal::compound_zero()),
            value: Some(Identifier::parse("return_value")?),
            marshaling,
            wire: true,
            void: false,
        })
    }
}

struct ClosureReturnValue<'render> {
    c_type: c::Type,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ReturnPlanRender<'plan, Native, IntoRust> for ClosureReturnValue<'render> {
    type Output = Result<ReturnValue>;

    fn void(&mut self) -> Self::Output {
        Ok(ReturnValue {
            c_type: TypeFragment::anonymous(&self.c_type)?,
            parser: None,
            default_value: Expression::literal(Literal::compound_zero()),
            value: None,
            marshaling: Marshaling::none(),
            wire: false,
            void: true,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Self::unsupported();
        }
        let direct = direct::NativeSlot::from_direct_value(ty, self.bridge, self.context)?;
        Ok(ReturnValue {
            c_type: TypeFragment::anonymous(&self.c_type)?,
            parser: Some(direct.parser().clone()),
            default_value: direct.default_value().clone(),
            value: Some(Identifier::parse("return_value")?),
            marshaling: Marshaling::direct(direct.primitive()),
            wire: false,
            void: false,
        })
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        codec: &'plan WritePlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::ReturnSlot, native::BufferShape::Buffer) => {
                ReturnValue::encoded(&self.c_type, codec)
            }
            (ReturnValueSlot::ReturnSlot, _) | (ReturnValueSlot::OutPointer, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown closure return",
            }),
        }
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        Self::unsupported()
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let primitive = primitive::Runtime::new(primitive);
        ReturnValue::wire(
            &self.c_type,
            primitive.optional_wire_encoder()?,
            Marshaling::wire(primitive),
        )
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let element = direct_vector::Element::from_element(element, self.bridge, self.context)?;
        ReturnValue::wire(
            &self.c_type,
            element.vector_encoder().clone(),
            Marshaling::direct_vector(element),
        )
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl<'render> ClosureReturnValue<'render> {
    fn unsupported() -> Result<ReturnValue> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure return",
        })
    }
}
