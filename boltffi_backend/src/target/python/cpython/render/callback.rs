use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackDecl, CallbackId, CanonicalName, ClosureReturn, DirectValueType,
    DirectVectorElementType, ErrorChannel, ErrorDecl, ErrorPlacement, ExecutionDecl,
    HandlePresence, HandleTarget, ImportedMethodDecl, IntoRust, Native, OutOfRust, OutgoingParam,
    ParamDecl, ParamPlanRender, Primitive, ReadPlan, ReturnPlan, ReturnPlanRender, ReturnValueSlot,
    TypeRef, VTableSlot, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Expression, Identifier, Literal, TypeFragment},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        codec::{BorrowedPayload, Marshaling, OwnedPayload},
        cpython::render::{direct, direct_vector, primitive},
        name_style::Name,
        syntax::Identifier as PythonIdentifier,
    },
};

use super::fallible::FallibleReturn;

#[derive(AskamaTemplate)]
#[template(path = "target/python/callback.c", escape = "none")]
struct Template {
    vtable_type: TypeFragment,
    vtable: Identifier,
    register: Identifier,
    register_storage: Identifier,
    create_handle_storage: Identifier,
    copy_buffer_storage: Identifier,
    parser: Identifier,
    optional_parser: Identifier,
    free: Identifier,
    clone: Identifier,
    slots: Vec<Slot>,
    methods: Vec<Method>,
}

pub struct Callback {
    symbols: Symbols,
    vtable_type: TypeFragment,
    register_storage: Identifier,
    create_handle_storage: Identifier,
    copy_buffer_storage: Identifier,
    slots: Vec<Slot>,
    methods: Vec<Method>,
}

impl Callback {
    pub fn from_declaration(
        declaration: &CallbackDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_callback =
            bridge
                .source_callback(declaration.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "callback without C bridge vtable",
                })?;
        let register = bridge
            .loaded_function(declaration.protocol().register())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback register symbol not loaded",
            })?;
        let create_handle = bridge
            .loaded_function(declaration.protocol().create_handle())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback handle constructor symbol not loaded",
            })?;
        let copy_buffer = bridge.buffer_from_bytes()?;
        let symbols = Symbols::from_declaration(declaration)?;
        let methods = declaration
            .protocol()
            .vtable()
            .methods()
            .iter()
            .map(|method| Method::new(method, c_callback, &symbols, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let slots = std::iter::once(Slot::new(
            Identifier::parse(declaration.protocol().vtable().free_slot().as_str())?,
            symbols.free().clone(),
        ))
        .chain(std::iter::once(Slot::new(
            Identifier::parse(declaration.protocol().vtable().clone_slot().as_str())?,
            symbols.clone().clone(),
        )))
        .chain(
            methods
                .iter()
                .map(|method| Slot::new(method.slot.clone(), method.function.clone())),
        )
        .collect();
        Ok(Self {
            symbols,
            vtable_type: TypeFragment::anonymous(&c::Type::named(c_callback.vtable().name())?)?,
            register_storage: register.storage_name().clone(),
            create_handle_storage: create_handle.storage_name().clone(),
            copy_buffer_storage: copy_buffer.storage_name().clone(),
            slots,
            methods,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        let source = Template {
            vtable_type: self.vtable_type,
            vtable: self.symbols.vtable,
            register: self.symbols.register,
            register_storage: self.register_storage,
            create_handle_storage: self.create_handle_storage,
            copy_buffer_storage: self.copy_buffer_storage,
            parser: self.symbols.parser,
            optional_parser: self.symbols.optional_parser,
            free: self.symbols.free,
            clone: self.symbols.clone,
            slots: self.slots,
            methods: self.methods,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    pub fn binding(&self) -> &str {
        self.symbols.register.as_str()
    }

    pub fn parser_declarations(&self) -> Vec<c::Statement> {
        self.symbols.parser_declarations().into_iter().collect()
    }

    pub fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.methods.iter().flat_map(Method::primitives)
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.methods.iter().flat_map(Method::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.methods.iter().flat_map(Method::direct_vector_elements)
    }

    pub fn has_string_argument(&self) -> bool {
        self.methods.iter().any(Method::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.methods.iter().any(Method::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.methods.iter().any(Method::has_raw_wire_argument)
    }
}

pub struct Symbols {
    parser: Identifier,
    optional_parser: Identifier,
    vtable: Identifier,
    register: Identifier,
    free: Identifier,
    clone: Identifier,
    method_prefix: String,
}

impl Symbols {
    pub fn from_callback_id(
        callback_id: CallbackId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let callback = context
            .callback(callback_id)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback id without declaration",
            })?;
        bridge
            .source_callback(callback_id)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback id without C bridge vtable",
            })?;
        Self::from_declaration(callback)
    }

    pub fn parser(&self, presence: HandlePresence) -> &Identifier {
        match presence {
            HandlePresence::Required => &self.parser,
            HandlePresence::Nullable => &self.optional_parser,
            _ => &self.parser,
        }
    }

    fn parser_declarations(&self) -> [c::Statement; 2] {
        [
            c::Statement::new(format!(
                "static int {}(PyObject *value, BoltFFICallbackHandle *out);",
                self.parser
            )),
            c::Statement::new(format!(
                "static int {}(PyObject *value, BoltFFICallbackHandle *out);",
                self.optional_parser
            )),
        ]
    }

    fn from_declaration(callback: &CallbackDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(callback.name()).function_text()?)?.to_string();
        let stem = format!("callback_{stem}");
        Ok(Self {
            parser: Identifier::parse(format!("boltffi_python_parse_{stem}"))?,
            optional_parser: Identifier::parse(format!("boltffi_python_parse_optional_{stem}"))?,
            vtable: Identifier::parse(format!("boltffi_python_{stem}_vtable"))?,
            register: Identifier::parse(format!("boltffi_python_bind_{stem}"))?,
            free: Identifier::parse(format!("boltffi_python_{stem}_free"))?,
            clone: Identifier::parse(format!("boltffi_python_{stem}_clone"))?,
            method_prefix: format!("boltffi_python_{stem}"),
        })
    }

    fn free(&self) -> &Identifier {
        &self.free
    }

    fn clone(&self) -> &Identifier {
        &self.clone
    }

    fn method(&self, name: &CanonicalName) -> Result<Identifier> {
        Identifier::parse(format!(
            "{}_{}",
            self.method_prefix,
            Identifier::escape(Name::new(name).function_text()?)?
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Slot {
    name: Identifier,
    function: Identifier,
}

impl Slot {
    fn new(name: Identifier, function: Identifier) -> Self {
        Self { name, function }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Method {
    slot: Identifier,
    function: Identifier,
    python_name: PythonIdentifier,
    returns: MethodReturn,
    fallible_return: Option<FallibleReturn>,
    completion: Option<AsyncCompletion>,
    wire_payload: bool,
    params: Vec<MethodParam>,
}

impl Method {
    fn new(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        c_callback: &c::Callback,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_field = c_callback
            .vtable()
            .fields()
            .iter()
            .find(|field| field.name() == method.target().as_str())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback method without C vtable slot",
            })?;
        let signature = MethodSignature::from_field(c_field)?;
        let arity = method
            .callable()
            .params()
            .iter()
            .map(MethodParam::arity)
            .collect::<Result<Vec<_>>>()?;
        let (c_params, fallible_return, completion) = match method.callable().execution() {
            ExecutionDecl::Synchronous(_) => match method.callable().error().channel() {
                ErrorChannel::None => (signature.value_params(&arity)?, None, None),
                ErrorChannel::Encoded {
                    placement: ErrorPlacement::ReturnSlot,
                    ..
                } => {
                    let parts = signature
                        .fallible_value_params(method.callable().returns().plan(), &arity)?;
                    let fallible_return = FallibleReturn::new(
                        method.callable().returns().plan(),
                        method.callable().error(),
                        parts.return_out.as_ref(),
                        bridge,
                        context,
                    )?;
                    (parts.params, Some(fallible_return), None)
                }
                _ => {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "callback method error channel",
                    });
                }
            },
            ExecutionDecl::Asynchronous(_) => {
                let parts = signature.async_value_params(&arity)?;
                let completion = AsyncCompletion::new(
                    method.callable().returns().plan(),
                    method.callable().error(),
                    &parts.completion,
                    &parts.completion_data,
                    bridge,
                    context,
                )?;
                (parts.params, None, Some(completion))
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown callback method execution",
                });
            }
        };
        let params = method
            .callable()
            .params()
            .iter()
            .zip(c_params.iter())
            .map(|(parameter, c_types)| MethodParam::new(parameter, c_types, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let returns = match &completion {
            Some(_) => MethodReturn::async_void(signature.returns())?,
            None if fallible_return.is_some() => MethodReturn::fallible_error(signature.returns())?,
            None => MethodReturn::new(
                method.callable().returns().plan(),
                signature.returns(),
                bridge,
                context,
            )?,
        };
        let wire_payload = returns.wire
            || fallible_return
                .as_ref()
                .is_some_and(FallibleReturn::uses_wire_payload)
            || completion
                .as_ref()
                .is_some_and(|completion| completion.payload.wire || completion.payload.error_wire);
        Ok(Self {
            slot: Identifier::parse(method.target().as_str())?,
            function: symbols.method(method.name())?,
            python_name: Name::new(method.name()).function()?,
            returns,
            fallible_return,
            completion,
            wire_payload,
            params,
        })
    }

    fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(|param| param.marshaling().primitive())
            .chain(self.returns.primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::primitives),
            )
            .chain(
                self.completion
                    .iter()
                    .filter_map(|completion| completion.payload.primitive()),
            )
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(|param| param.marshaling().wire_primitive())
            .chain(self.returns.wire_primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::wire_primitives),
            )
            .chain(
                self.completion
                    .iter()
                    .filter_map(|completion| completion.payload.wire_primitive()),
            )
    }

    fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(|param| param.marshaling().direct_vector_element())
            .chain(self.returns.direct_vector())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::direct_vectors),
            )
            .chain(
                self.completion
                    .iter()
                    .filter_map(|completion| completion.payload.direct_vector()),
            )
    }

    fn has_string_argument(&self) -> bool {
        self.params
            .iter()
            .any(|param| param.marshaling().has_string())
            || self.returns.has_string()
            || self.fallible_return.iter().any(FallibleReturn::has_string)
            || self
                .completion
                .iter()
                .any(|completion| completion.payload.has_string())
    }

    fn has_bytes_argument(&self) -> bool {
        self.params
            .iter()
            .any(|param| param.marshaling().has_bytes())
            || self.returns.has_bytes()
            || self.fallible_return.iter().any(FallibleReturn::has_bytes)
            || self
                .completion
                .iter()
                .any(|completion| completion.payload.has_bytes())
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params
            .iter()
            .any(|param| param.marshaling().has_raw_wire())
            || self.returns.has_raw_wire()
            || self
                .fallible_return
                .iter()
                .any(FallibleReturn::has_raw_wire)
            || self
                .completion
                .iter()
                .any(|completion| completion.payload.has_raw_wire())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodSignature {
    returns: c::Type,
    params: Vec<c::Type>,
}

impl MethodSignature {
    fn from_field(field: &c::Field) -> Result<Self> {
        match field.ty() {
            c::Type::FunctionPointer { returns, params } => Ok(Self {
                returns: returns.as_ref().clone(),
                params: params.clone(),
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback vtable slot is not a function pointer",
            }),
        }
    }

    fn returns(&self) -> &c::Type {
        &self.returns
    }

    fn value_params(&self, arity: &[usize]) -> Result<Vec<Vec<c::Type>>> {
        let value_param_count = arity.iter().sum::<usize>();
        let value_start =
            self.params
                .len()
                .checked_sub(value_param_count)
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "callback method parameter ABI mismatch",
                })?;
        if value_start == 0 {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback method handle ABI mismatch",
            });
        }
        Ok(arity
            .iter()
            .scan(value_start, |offset, count| {
                let start = *offset;
                *offset += *count;
                Some(self.params[start..*offset].to_vec())
            })
            .collect())
    }

    fn async_value_params(&self, arity: &[usize]) -> Result<AsyncSignature> {
        let value_param_count = arity.iter().sum::<usize>();
        let value_start = 1;
        let value_end = value_start + value_param_count;
        let completion_index = value_end;
        let completion_data_index = completion_index + 1;
        if self.params.len() != completion_data_index + 1 {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback method parameter ABI mismatch",
            });
        }
        if !matches!(&self.returns, c::Type::Void) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback method return ABI mismatch",
            });
        }
        Ok(AsyncSignature {
            params: arity
                .iter()
                .scan(value_start, |offset, count| {
                    let start = *offset;
                    *offset += *count;
                    Some(self.params[start..*offset].to_vec())
                })
                .collect(),
            completion: self.params[completion_index].clone(),
            completion_data: self.params[completion_data_index].clone(),
        })
    }

    fn fallible_value_params(
        &self,
        plan: &ReturnPlan<Native, IntoRust>,
        arity: &[usize],
    ) -> Result<FallibleSignature> {
        let return_param_count = Self::return_param_count(plan)?;
        let value_param_count = arity.iter().sum::<usize>();
        let value_start = 1 + return_param_count;
        if self.params.len() != value_start + value_param_count {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible callback method parameter ABI mismatch",
            });
        }
        Ok(FallibleSignature {
            return_out: (return_param_count == 1).then(|| self.params[1].clone()),
            params: arity
                .iter()
                .scan(value_start, |offset, count| {
                    let start = *offset;
                    *offset += *count;
                    Some(self.params[start..*offset].to_vec())
                })
                .collect(),
        })
    }

    fn return_param_count(plan: &ReturnPlan<Native, IntoRust>) -> Result<usize> {
        plan.render_with(&mut CallbackSuccessOutCount)
    }
}

struct CallbackSuccessOutCount;

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for CallbackSuccessOutCount {
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
        Self::unsupported()
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Self::unsupported()
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl CallbackSuccessOutCount {
    fn slot_count(slot: ReturnValueSlot) -> Result<usize> {
        match slot {
            ReturnValueSlot::OutPointer => Ok(1),
            ReturnValueSlot::ReturnSlot => Self::unsupported(),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible callback success slot",
            }),
        }
    }

    fn unsupported() -> Result<usize> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported fallible callback success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsyncSignature {
    params: Vec<Vec<c::Type>>,
    completion: c::Type,
    completion_data: c::Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleSignature {
    params: Vec<Vec<c::Type>>,
    return_out: Option<c::Type>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodParam {
    declarations: Vec<c::Statement>,
    name: Identifier,
    object: Identifier,
    expression: c::Expression,
    marshaling: Marshaling,
}

impl MethodParam {
    fn arity(parameter: &ParamDecl<Native, OutOfRust>) -> Result<usize> {
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut MethodParamArity),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method parameter",
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
            OutgoingParam::Value(plan) => plan.render_with(&mut MethodParamValue {
                name,
                object,
                c_types: c_types.to_vec(),
                bridge,
                context,
            }),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method parameter",
            }),
        }
    }

    fn marshaling(&self) -> &Marshaling {
        &self.marshaling
    }

    fn direct(
        name: Identifier,
        object: Identifier,
        ty: &DirectValueType,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if c_types.len() != 1 {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback direct parameter ABI mismatch",
            });
        }
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        let expression = direct.box_expression(name.clone());
        Ok(Self {
            declarations: vec![TypeFragment::declaration(&c_types[0], name.as_str())?],
            name,
            object,
            expression,
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
                shape: "callback encoded parameter ABI mismatch",
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
            name,
            object,
            expression,
            marshaling,
        })
    }

    fn direct_vector_param(
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
                shape: "callback vector parameter ABI mismatch",
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
            name,
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

struct MethodParamArity;

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for MethodParamArity {
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
                shape: "unsupported callback method parameter",
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
        Ok(1)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported callback method parameter",
        })
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType, _: ()) -> Self::Output {
        Ok(2)
    }
}

struct MethodParamValue<'render> {
    name: Identifier,
    object: Identifier,
    c_types: Vec<c::Type>,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ParamPlanRender<'plan, Native, OutOfRust> for MethodParamValue<'render> {
    type Output = Result<MethodParam>;

    fn direct(&mut self, ty: &DirectValueType, _: ()) -> Self::Output {
        MethodParam::direct(
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
                MethodParam::encoded(self.name.clone(), self.object.clone(), codec, &self.c_types)
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported callback method encoded parameter",
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
            shape: "callback handle method parameter",
        })
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unknown callback method parameter",
        })
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType, _: ()) -> Self::Output {
        MethodParam::direct_vector_param(
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
struct AsyncCompletion {
    declaration: c::Statement,
    data_declaration: c::Statement,
    callback: Identifier,
    data: Identifier,
    payload: CompletionPayload,
}

impl AsyncCompletion {
    fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        error: &ErrorDecl<Native, IntoRust>,
        completion: &c::Type,
        completion_data: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let signature = CompletionSignature::new(completion)?;
        let payload = match error.channel() {
            ErrorChannel::None => {
                CompletionPayload::infallible(plan, signature.payload(), bridge, context)?
            }
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                codec,
                ..
            } => CompletionPayload::fallible(plan, codec, signature.payload(), bridge, context)?,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "async callback error channel",
                });
            }
        };
        Ok(Self {
            declaration: TypeFragment::declaration(completion, "completion")?,
            data_declaration: TypeFragment::declaration(completion_data, "completion_data")?,
            callback: Identifier::parse("completion")?,
            data: Identifier::parse("completion_data")?,
            payload,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionSignature {
    payload: Option<c::Type>,
}

impl CompletionSignature {
    fn new(completion: &c::Type) -> Result<Self> {
        let c::Type::FunctionPointer { returns, params } = completion else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback completion is not a function pointer",
            });
        };
        if !matches!(returns.as_ref(), c::Type::Void) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback completion return ABI mismatch",
            });
        }
        if !matches!(
            params.as_slice(),
            [c::Type::MutPointer(data), c::Type::Status]
                if matches!(data.as_ref(), c::Type::Void)
        ) && !matches!(
            params.as_slice(),
            [c::Type::MutPointer(data), c::Type::Status, _]
                if matches!(data.as_ref(), c::Type::Void)
        ) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback completion parameter ABI mismatch",
            });
        }
        Ok(Self {
            payload: params.get(2).cloned(),
        })
    }

    fn payload(&self) -> Option<&c::Type> {
        self.payload.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionPayload {
    value: Option<Identifier>,
    c_type: Option<TypeFragment>,
    default_value: Option<Expression>,
    parser: Option<Identifier>,
    error_parser: Option<Identifier>,
    direct_value: Option<Identifier>,
    direct_type: Option<TypeFragment>,
    error_direct_value: Option<Identifier>,
    error_direct_type: Option<TypeFragment>,
    wire: bool,
    direct_bytes: bool,
    error_wire: bool,
    error_direct_bytes: bool,
    fallible: bool,
    void: bool,
    marshaling: Marshaling,
}

impl CompletionPayload {
    fn infallible(
        plan: &ReturnPlan<Native, IntoRust>,
        payload: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut AsyncCompletionPayload {
            payload: payload.cloned(),
            bridge,
            context,
        })
    }

    fn fallible(
        success: &ReturnPlan<Native, IntoRust>,
        error: &WritePlan,
        payload: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let payload = payload.ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "async fallible callback completion without payload",
        })?;
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async fallible callback payload ABI mismatch",
            });
        }
        let success = Self::fallible_success(success, payload, bridge, context)?;
        let error = Self::wire(error, payload)?;
        Ok(Self {
            error_parser: error.parser,
            error_direct_value: Some(Identifier::parse("completion_error_direct_value")?),
            error_direct_type: error.direct_type,
            error_wire: error.wire,
            error_direct_bytes: error.direct_bytes,
            fallible: true,
            ..success
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

    fn value(&self) -> &Identifier {
        self.value
            .as_ref()
            .expect("completion payload value is present")
    }

    fn c_type(&self) -> &TypeFragment {
        self.c_type
            .as_ref()
            .expect("completion payload C type is present")
    }

    fn default_value(&self) -> &Expression {
        self.default_value
            .as_ref()
            .expect("completion payload default value is present")
    }

    fn parser(&self) -> &Identifier {
        self.parser
            .as_ref()
            .expect("completion payload parser is present")
    }

    fn error_parser(&self) -> &Identifier {
        self.error_parser
            .as_ref()
            .expect("completion payload error parser is present")
    }

    fn direct_value(&self) -> &Identifier {
        self.direct_value
            .as_ref()
            .expect("completion payload direct value is present")
    }

    fn direct_type(&self) -> &TypeFragment {
        self.direct_type
            .as_ref()
            .expect("completion payload direct type is present")
    }

    fn error_direct_value(&self) -> &Identifier {
        self.error_direct_value
            .as_ref()
            .expect("completion payload error direct value is present")
    }

    fn error_direct_type(&self) -> &TypeFragment {
        self.error_direct_type
            .as_ref()
            .expect("completion payload error direct type is present")
    }

    fn void() -> Self {
        Self {
            value: None,
            c_type: None,
            default_value: None,
            parser: None,
            error_parser: None,
            direct_value: None,
            direct_type: None,
            error_direct_value: None,
            error_direct_type: None,
            wire: false,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: true,
            marshaling: Marshaling::none(),
        }
    }

    fn direct(
        ty: &DirectValueType,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        Self {
            value: Some(Identifier::parse("completion_value")?),
            c_type: None,
            default_value: Some(direct.default_value().clone()),
            parser: Some(direct.parser().clone()),
            error_parser: None,
            direct_value: None,
            direct_type: None,
            error_direct_value: None,
            error_direct_type: None,
            wire: false,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            marshaling: Marshaling::direct(direct.primitive()),
        }
        .with_payload_type(payload)
    }

    fn wire(codec: &WritePlan, payload: &c::Type) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async wire callback payload ABI mismatch",
            });
        }
        let encoded = OwnedPayload::write(codec)?;
        Self {
            value: Some(Identifier::parse("completion_value")?),
            c_type: None,
            default_value: Some(Expression::literal(Literal::compound_zero())),
            parser: Some(encoded.parser().clone()),
            error_parser: None,
            direct_value: None,
            direct_type: None,
            error_direct_value: None,
            error_direct_type: None,
            wire: true,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            marshaling: encoded.marshaling(),
        }
        .with_payload_type(payload)
    }

    fn vector(
        element: &DirectVectorElementType,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async vector callback payload ABI mismatch",
            });
        }
        let element = direct_vector::Element::from_element(element, bridge, context)?;
        Self {
            value: Some(Identifier::parse("completion_value")?),
            c_type: None,
            default_value: Some(Expression::literal(Literal::compound_zero())),
            parser: Some(element.vector_encoder().clone()),
            error_parser: None,
            direct_value: None,
            direct_type: None,
            error_direct_value: None,
            error_direct_type: None,
            wire: true,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            marshaling: Marshaling::direct_vector(element),
        }
        .with_payload_type(payload)
    }

    fn scalar_option(primitive: Primitive, payload: &c::Type) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async optional callback payload ABI mismatch",
            });
        }
        let primitive = primitive::Runtime::new(primitive);
        Self {
            value: Some(Identifier::parse("completion_value")?),
            c_type: None,
            default_value: Some(Expression::literal(Literal::compound_zero())),
            parser: Some(primitive.optional_wire_encoder()?),
            error_parser: None,
            direct_value: None,
            direct_type: None,
            error_direct_value: None,
            error_direct_type: None,
            wire: true,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            marshaling: Marshaling::wire(primitive),
        }
        .with_payload_type(payload)
    }

    fn fallible_success(
        plan: &ReturnPlan<Native, IntoRust>,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut AsyncCompletionSuccess {
            payload: payload.clone(),
            bridge,
            context,
        })
    }

    fn direct_bytes(
        ty: &DirectValueType,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async direct-byte callback payload ABI mismatch",
            });
        }
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        Self {
            value: Some(Identifier::parse("completion_value")?),
            c_type: None,
            default_value: Some(Expression::literal(Literal::compound_zero())),
            parser: Some(direct.parser().clone()),
            error_parser: None,
            direct_value: Some(Identifier::parse("completion_direct_value")?),
            direct_type: Some(direct.c_type().clone()),
            error_direct_value: None,
            error_direct_type: None,
            wire: false,
            direct_bytes: true,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            marshaling: Marshaling::none(),
        }
        .with_payload_type(payload)
    }

    fn wire_empty(payload: &c::Type) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async empty callback payload ABI mismatch",
            });
        }
        Self {
            value: Some(Identifier::parse("completion_value")?),
            c_type: None,
            default_value: Some(Expression::literal(Literal::compound_zero())),
            parser: None,
            error_parser: None,
            direct_value: None,
            direct_type: None,
            error_direct_value: None,
            error_direct_type: None,
            wire: false,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            marshaling: Marshaling::none(),
        }
        .with_payload_type(payload)
    }

    fn with_payload_type(mut self, payload: &c::Type) -> Result<Self> {
        let payload_type = TypeFragment::anonymous(payload)?;
        let zero = Expression::literal(Literal::compound_zero());
        self.c_type = Some(payload_type.clone());
        if self.default_value.as_ref() == Some(&zero) {
            self.default_value = Some(Expression::cast(payload_type, zero));
        }
        Ok(self)
    }
}

struct AsyncCompletionPayload<'render> {
    payload: Option<c::Type>,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ReturnPlanRender<'plan, Native, IntoRust> for AsyncCompletionPayload<'render> {
    type Output = Result<CompletionPayload>;

    fn void(&mut self) -> Self::Output {
        if self.payload.is_some() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async void callback completion payload",
            });
        }
        Ok(CompletionPayload::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Self::unsupported();
        }
        CompletionPayload::direct(
            ty,
            self.payload.as_ref().ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "async direct callback completion without payload",
            })?,
            self.bridge,
            self.context,
        )
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        codec: &'plan WritePlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::ReturnSlot, native::BufferShape::Buffer) => CompletionPayload::wire(
                codec,
                self.payload.as_ref().ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "async encoded callback completion without payload",
                })?,
            ),
            (ReturnValueSlot::ReturnSlot, _) | (ReturnValueSlot::OutPointer, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async callback return slot",
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
        CompletionPayload::scalar_option(
            primitive,
            self.payload.as_ref().ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "async optional callback completion without payload",
            })?,
        )
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        CompletionPayload::vector(
            element,
            self.payload.as_ref().ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "async vector callback completion without payload",
            })?,
            self.bridge,
            self.context,
        )
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl<'render> AsyncCompletionPayload<'render> {
    fn unsupported() -> Result<CompletionPayload> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported async callback return",
        })
    }
}

struct AsyncCompletionSuccess<'render> {
    payload: c::Type,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ReturnPlanRender<'plan, Native, IntoRust> for AsyncCompletionSuccess<'render> {
    type Output = Result<CompletionPayload>;

    fn void(&mut self) -> Self::Output {
        CompletionPayload::wire_empty(&self.payload)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => {
                CompletionPayload::direct_bytes(ty, &self.payload, self.bridge, self.context)
            }
            ReturnValueSlot::ReturnSlot => Self::unsupported(),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async fallible callback success slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        codec: &'plan WritePlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::OutPointer, native::BufferShape::Buffer) => {
                CompletionPayload::wire(codec, &self.payload)
            }
            (ReturnValueSlot::OutPointer, _) | (ReturnValueSlot::ReturnSlot, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async fallible callback success slot",
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

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Self::unsupported()
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Self::unsupported()
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl<'render> AsyncCompletionSuccess<'render> {
    fn unsupported() -> Result<CompletionPayload> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported async fallible callback success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodReturn {
    c_type: TypeFragment,
    parser: Option<Identifier>,
    default_value: Expression,
    value: Option<Identifier>,
    marshaling: Marshaling,
    wire: bool,
    void: bool,
}

impl MethodReturn {
    fn async_void(c_type: &c::Type) -> Result<Self> {
        Ok(Self {
            c_type: TypeFragment::anonymous(c_type)?,
            parser: None,
            default_value: Expression::literal(Literal::compound_zero()),
            value: None,
            marshaling: Marshaling::none(),
            wire: false,
            void: true,
        })
    }

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
        plan.render_with(&mut CallbackMethodReturnValue {
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
        self.parser.as_ref().expect("callback return parser")
    }

    fn value(&self) -> &Identifier {
        self.value.as_ref().expect("callback return value")
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

struct CallbackMethodReturnValue<'render> {
    c_type: c::Type,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ReturnPlanRender<'plan, Native, IntoRust>
    for CallbackMethodReturnValue<'render>
{
    type Output = Result<MethodReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(MethodReturn {
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
        Ok(MethodReturn {
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
                MethodReturn::encoded(&self.c_type, codec)
            }
            (ReturnValueSlot::ReturnSlot, _) | (ReturnValueSlot::OutPointer, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method return",
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
        MethodReturn::wire(
            &self.c_type,
            primitive.optional_wire_encoder()?,
            Marshaling::wire(primitive),
        )
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let element = direct_vector::Element::from_element(element, self.bridge, self.context)?;
        MethodReturn::wire(
            &self.c_type,
            element.vector_encoder().clone(),
            Marshaling::direct_vector(element),
        )
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl<'render> CallbackMethodReturnValue<'render> {
    fn unsupported() -> Result<MethodReturn> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported callback method return",
        })
    }
}
