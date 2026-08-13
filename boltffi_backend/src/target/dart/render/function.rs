use askama::Template;
use boltffi_binding::{
    DirectValueType, DirectVectorElementType, ErrorDecl, ExecutionDecl, ExportedCallable,
    HandlePresence, HandleTarget, IncomingParam, Native, NativeSymbol, ParamPlan, ReadPlan,
    Receive, ReturnPlan, TypeRef, native as binding_native,
};

use crate::{
    bridge::c::{CBridgeContract, Function as CFunction, ParameterGroup, Type as CBridgeType},
    core::{Emitted, Error, RenderContext, Result},
};

use super::super::{
    codec::{Reader, Sizer, ValueScope, Writer},
    name_style::Name,
    native::{self as dart_native, NativeCallableSource, NativeParameterSource},
    syntax::{Identifier, Parameter, TypeFragment},
    type_name,
};
use super::{
    Documentation, closure::ClosureArgument, direct_vector::PrimitiveVector, indent,
    returned_closure::ReturnedClosure,
};

#[derive(Template)]
#[template(path = "target/dart/function.dart", escape = "none")]
struct FunctionTemplate<'a> {
    function: &'a Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Placement {
    TopLevel,
    Static,
    Getter { associated: bool },
    Instance(Receiver),
    Initializer { owner: Identifier, primary: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Receiver {
    Class,
    DirectValue(DirectValueType),
    EncodedValue,
}

pub struct Function {
    documentation: Documentation,
    name: Identifier,
    parameters: Vec<Parameter>,
    return_type: TypeFragment,
    placement: FunctionPlacement,
    body: String,
}

enum FunctionPlacement {
    TopLevel,
    Static,
    Getter { associated: bool },
    Instance,
    Factory {
        owner: Identifier,
        constructor: Option<Identifier>,
    },
}

pub struct DartParameter {
    signature: Parameter,
    argument: DartArgument,
}

struct DartArgument {
    setup: Vec<String>,
    native_arguments: Vec<String>,
    writeback: Vec<String>,
    cleanup: Vec<String>,
}

pub struct DartReturn {
    pub public_type: TypeFragment,
    pub before_call: Vec<String>,
    pub arguments: Vec<String>,
    pub call_result: Option<String>,
    pub after_call: Vec<String>,
    pub expression: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OutPointer {
    ty: CBridgeType,
}

impl OutPointer {
    fn from_index(
        index: crate::bridge::c::ParameterIndex,
        function: &impl NativeParameterSource,
    ) -> Result<Self> {
        match function.parameter(index).ty() {
            CBridgeType::MutPointer(inner) => Ok(Self {
                ty: inner.as_ref().clone(),
            }),
            _ => broken("Dart out storage is not a mutable C pointer"),
        }
    }

    fn allocation(&self, name: &str) -> Result<String> {
        let native = dart_native::NativeType::from_c(&self.ty)?;
        Ok(format!(
            "final {name} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>());",
            native.native(),
            native.native(),
        ))
    }

    // Skips the `NativeFinalizer` registration `.alloc()` pays on every call
    // -- only safe when the caller disposes the storage itself before it
    // goes out of scope, which is guaranteed for storage that's read and
    // discarded within a single synchronous statement sequence (e.g. a
    // completion status).
    fn allocation_unmanaged(&self, name: &str) -> Result<String> {
        let native = dart_native::NativeType::from_c(&self.ty)?;
        Ok(format!(
            "final {name} = _$$BoltCallocPtr<{}>.allocUnmanaged($$ffi.sizeOf<{}>());",
            native.native(),
            native.native(),
        ))
    }

    fn read(&self, pointer: &str) -> Result<String> {
        dart_native::pointer_read(&self.ty, pointer)
    }
}

impl Function {
    #[allow(clippy::too_many_arguments)]
    pub fn from_callable(
        name: &boltffi_binding::CanonicalName,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        placement: Placement,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
        doc: Option<&boltffi_binding::DocComment>,
    ) -> Result<Self> {
        let name = Name::new(name).lower_camel()?;
        let start_function = dart_native::bridge_function(symbol, bridge.functions())?;
        let completion = match callable.execution() {
            ExecutionDecl::Synchronous(_) => None,
            ExecutionDecl::Asynchronous(binding_native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                ..
            }) => Some(AsyncFunctions {
                poll,
                cancel,
                free,
                completion: dart_native::bridge_function(complete, bridge.functions())?,
            }),
            ExecutionDecl::Asynchronous(_) => {
                return super::super::unsupported("async protocol other than poll handle");
            }
            _ => return super::super::unsupported("unknown callable execution protocol"),
        };

        let mut group_index = 0;
        let mut receiver_setup = Vec::new();
        let mut receiver_cleanup = Vec::new();
        let mut arguments = Vec::new();
        if callable.receiver().is_some() {
            let receiver = match &placement {
                Placement::Instance(receiver) => receiver,
                _ => return super::super::unsupported("callable receiver placement"),
            };
            let group = start_function.parameter_groups().get(group_index).ok_or(
                Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "Dart method receiver is missing from the C bridge",
                },
            )?;
            group_index += 1;
            let receiver_argument = render_receiver(
                receiver,
                callable.receiver().expect("receiver was checked"),
                group,
                start_function,
                context,
            )?;
            receiver_setup.extend(receiver_argument.setup);
            arguments.extend(receiver_argument.native_arguments);
            receiver_cleanup.extend(receiver_argument.writeback);
            receiver_cleanup.extend(receiver_argument.cleanup);
        }

        let parameters = callable
            .params()
            .iter()
            .map(|parameter| {
                let group = start_function.parameter_groups().get(group_index).ok_or(
                    Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "Dart parameter is missing from the C bridge",
                    },
                )?;
                group_index += 1;
                match parameter.payload() {
                    IncomingParam::Value(plan) => render_parameter(
                        Name::new(parameter.name()).lower_camel()?,
                        plan,
                        group,
                        start_function,
                        bridge,
                        context,
                    ),
                    IncomingParam::Closure(closure) => {
                        let ParameterGroup::Closure(protocol) = group else {
                            return broken("Dart closure parameter disagrees with C bridge group");
                        };
                        let closure = ClosureArgument::from_declaration(
                            parameter.name(),
                            closure,
                            protocol,
                            start_function,
                            bridge,
                            context,
                        )?;
                        Ok(DartParameter::new(
                            closure.name,
                            closure.public_type,
                            DartArgument::new(closure.setup, closure.arguments, Vec::new()),
                        ))
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        let (return_function, return_groups) = completion.as_ref().map_or_else(
            || {
                (
                    start_function,
                    &start_function.parameter_groups()[group_index..],
                )
            },
            |asynchronous| {
                (
                    asynchronous.completion,
                    &asynchronous.completion.parameter_groups()[1..],
                )
            },
        );
        let returns = render_return(
            callable.returns().plan(),
            callable.error(),
            return_function,
            return_groups,
            bridge,
            context,
        )?;

        let declarations = parameters
            .iter()
            .map(|parameter| parameter.signature.clone())
            .collect::<Vec<_>>();
        receiver_setup.extend(
            parameters
                .iter()
                .flat_map(|parameter| parameter.argument.setup.iter().cloned()),
        );
        arguments.extend(
            parameters
                .iter()
                .flat_map(|parameter| parameter.argument.native_arguments.iter().cloned()),
        );
        let cleanup = receiver_cleanup
            .into_iter()
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.argument.writeback.iter().cloned()),
            )
            .chain(
                parameters
                    .iter()
                    .flat_map(|parameter| parameter.argument.cleanup.iter().cloned()),
            )
            .collect::<Vec<_>>();

        let asynchronous = completion.is_some();
        let public_return_type = match asynchronous {
            true => returns.public_type.clone().future(),
            false => returns.public_type.clone(),
        };
        let placement = match placement {
            Placement::TopLevel => FunctionPlacement::TopLevel,
            Placement::Static => FunctionPlacement::Static,
            Placement::Getter { associated } => FunctionPlacement::Getter { associated },
            Placement::Instance(_) => FunctionPlacement::Instance,
            Placement::Initializer { owner, .. }
                if !asynchronous && returns.public_type.as_str() == owner.as_str() =>
            {
                FunctionPlacement::Factory {
                    owner,
                    constructor: factory_constructor_name(&name),
                }
            }
            Placement::Initializer { .. } => FunctionPlacement::Static,
        };

        let call = match completion {
            Some(asynchronous) => render_async_call(
                start_function,
                asynchronous,
                &arguments,
                &receiver_setup,
                &cleanup,
                &returns,
            )?,
            None => render_sync_call(
                start_function,
                &arguments,
                &receiver_setup,
                &cleanup,
                &returns,
            ),
        };
        Ok(Self {
            documentation: Documentation::new(doc, 0),
            name,
            parameters: declarations,
            return_type: public_return_type,
            placement,
            body: indent(&call, 2),
        })
    }

    pub fn source(&self) -> String {
        FunctionTemplate { function: self }
            .render()
            .expect("rendering an in-memory Dart function template cannot fail")
    }

    pub fn render(self) -> Emitted {
        Emitted::primary([self.source(), "\n".to_owned()].concat())
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    fn return_type(&self) -> &TypeFragment {
        &self.return_type
    }

    fn placement(&self) -> &FunctionPlacement {
        &self.placement
    }

    fn body(&self) -> &str {
        &self.body
    }
}

impl DartParameter {
    fn new(name: Identifier, ty: TypeFragment, argument: DartArgument) -> Self {
        Self {
            signature: Parameter::new(name, ty),
            argument,
        }
    }

    pub fn public_type(&self) -> &TypeFragment {
        self.signature.ty()
    }

    pub fn signature(&self) -> &Parameter {
        &self.signature
    }

    pub fn setup(&self) -> &[String] {
        &self.argument.setup
    }

    pub fn native_arguments(&self) -> &[String] {
        &self.argument.native_arguments
    }

    pub fn writeback(&self) -> &[String] {
        &self.argument.writeback
    }

    pub fn cleanup(&self) -> &[String] {
        &self.argument.cleanup
    }
}

impl DartArgument {
    fn new(setup: Vec<String>, native_arguments: Vec<String>, writeback: Vec<String>) -> Self {
        Self {
            setup,
            native_arguments,
            writeback,
            cleanup: Vec::new(),
        }
    }

    fn with_cleanup(
        setup: Vec<String>,
        native_arguments: Vec<String>,
        cleanup: Vec<String>,
    ) -> Self {
        Self {
            setup,
            native_arguments,
            writeback: Vec::new(),
            cleanup,
        }
    }
}

impl FunctionPlacement {
    fn factory(&self) -> bool {
        matches!(self, Self::Factory { .. })
    }

    fn owner(&self) -> &Identifier {
        match self {
            Self::Factory { owner, .. } => owner,
            _ => unreachable!("template guards Dart factory owner access"),
        }
    }

    fn constructor(&self) -> Option<&Identifier> {
        match self {
            Self::Factory { constructor, .. } => constructor.as_ref(),
            _ => None,
        }
    }

    fn static_keyword(&self) -> &'static str {
        match self {
            Self::Static | Self::Getter { associated: true } => "static ",
            Self::TopLevel
            | Self::Getter { associated: false }
            | Self::Instance
            | Self::Factory { .. } => "",
        }
    }

    fn getter_keyword(&self) -> &'static str {
        match self {
            Self::Getter { .. } => "get ",
            _ => "",
        }
    }

    fn getter(&self) -> bool {
        matches!(self, Self::Getter { .. })
    }
}

pub fn associated_functions(
    initializers: &[boltffi_binding::InitializerDecl<Native>],
    methods: &[boltffi_binding::ExportedMethodDecl<Native, NativeSymbol>],
    initializer_placement: Placement,
    receiver: Receiver,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<Function>> {
    let initializers = initializers.iter().map(|initializer| {
        Function::from_callable(
            initializer.name(),
            initializer.symbol(),
            initializer.callable(),
            initializer_placement.clone(),
            bridge,
            context,
            initializer.meta().doc(),
        )
    });
    let methods = methods.iter().map(|method| {
        let placement = match method.callable().receiver() {
            Some(_) => Placement::Instance(receiver.clone()),
            None => Placement::Static,
        };
        Function::from_callable(
            method.name(),
            method.target(),
            method.callable(),
            placement,
            bridge,
            context,
            method.meta().doc(),
        )
    });
    initializers.chain(methods).collect()
}

struct AsyncFunctions<'bridge> {
    poll: &'bridge NativeSymbol,
    cancel: &'bridge NativeSymbol,
    free: &'bridge NativeSymbol,
    completion: &'bridge CFunction,
}

pub fn render_parameter(
    name: Identifier,
    plan: &ParamPlan<Native, boltffi_binding::IntoRust>,
    group: &ParameterGroup,
    function: &impl NativeParameterSource,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<DartParameter> {
    match plan {
        ParamPlan::Direct { ty, receive } => {
            let public_type = type_name::direct_value(ty, context)?;
            let argument = render_direct_argument(name.as_str(), ty, *receive, group, function)?;
            Ok(DartParameter::new(name, public_type, argument))
        }
        ParamPlan::Encoded { ty, codec, .. } => {
            let mutable = match group {
                ParameterGroup::ByteSlice(_) => false,
                ParameterGroup::EncodedWriteback(_) => true,
                _ => return broken("encoded Dart parameter disagrees with C bridge group"),
            };
            if mutable {
                return super::super::unsupported("mutable encoded parameter");
            }
            let storage = format!("_l${}Storage", name.as_str());
            let writer = format!("_l${}Writer", name.as_str());
            let size = codec
                .size_with(&mut Sizer::new(
                    ValueScope::current(name.to_string()),
                    context,
                ))?
                .into_source();
            let writes = codec
                .render_with(&mut Writer::new(
                    &writer,
                    ValueScope::current(name.to_string()),
                    context,
                ))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .map(super::super::codec::WriteStatement::into_source)
                .collect::<Vec<_>>();
            let public_type = type_name::type_ref(ty, context)?;
            Ok(DartParameter::new(
                name,
                public_type,
                DartArgument::with_cleanup(
                std::iter::once(format!(
                    "final {storage} = _$$BoltStoragePool.acquireStorage({size});"
                ))
                .chain(std::iter::once(format!(
                    "final {writer} = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({storage}.ptr, {storage}.len));"
                )))
                .chain(writes)
                .collect(),
                vec![
                    format!("{storage}.ptr"),
                    format!("{writer}.len"),
                ],
                vec![format!("_$$BoltStoragePool.releaseStorage({storage});")],
                ),
            ))
        }
        ParamPlan::Handle {
            target, presence, ..
        } => {
            let ParameterGroup::Value(_) = group else {
                return broken("handle Dart parameter disagrees with C bridge group");
            };
            let argument = match target {
                HandleTarget::Class(_) => match presence {
                    HandlePresence::Required => format!("{name}._handle"),
                    HandlePresence::Nullable => format!("{name}?._handle ?? 0"),
                    _ => return super::super::unsupported("unknown handle presence"),
                },
                HandleTarget::Callback(_) => {
                    let callback = type_name::handle(target, HandlePresence::Required, context)?;
                    format!("{callback}Bridge.create({name})")
                }
                HandleTarget::Stream(_) => {
                    return super::super::unsupported("stream handle parameter");
                }
                _ => return super::super::unsupported("unknown handle parameter"),
            };
            let public_type = type_name::handle(target, *presence, context)?;
            Ok(DartParameter::new(
                name,
                public_type,
                DartArgument::new(Vec::new(), vec![argument], Vec::new()),
            ))
        }
        ParamPlan::DirectVec { element, receive } => {
            let ParameterGroup::DirectVector(_group) = group else {
                return broken("direct-vector Dart parameter disagrees with C bridge group");
            };
            let storage = format!("_l${}Storage", name.as_str());
            match element {
                DirectVectorElementType::Primitive(primitive) => {
                    let primitive = primitive.primitive();
                    let vector = PrimitiveVector::new(primitive)?;
                    let native_type = vector.native();
                    let populate = vector.populate(&storage, name.as_str())?;
                    let public_type = type_name::direct_vector(element, context)?;
                    let writeback = match receive {
                        Receive::ByMutRef => {
                            vec![vector.writeback(&storage, name.as_str())?]
                        }
                        _ => Vec::new(),
                    };
                    let argument = DartArgument::new(
                        vec![
                            format!(
                                "final {storage} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>() * {name}.length);",
                                native_type.native(),
                                native_type.native()
                            ),
                            populate,
                        ],
                        vec![format!("{storage}.ptr"), format!("{name}.length")],
                        writeback,
                    );
                    Ok(DartParameter::new(name, public_type, argument))
                }
                DirectVectorElementType::Record(record) => {
                    let direct = DirectValueType::Record(*record);
                    let public = type_name::direct_value(&direct, context)?;
                    let native = dart_native::direct_record_struct(bridge, *record)?;
                    let argument = DartArgument::new(
                        vec![
                            format!(
                                "final {storage} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>() * {name}.length);",
                                native, native
                            ),
                            format!(
                                "for (var _l$index = 0; _l$index < {name}.length; _l$index++) {{ {name}[_l$index]._m$writeStruct({storage}.ptr.elementAt(_l$index)); }}"
                            ),
                        ],
                        vec![
                            format!("{storage}.ptr.cast<$$ffi.Uint8>()"),
                            format!("{name}.length * $$ffi.sizeOf<{native}>()"),
                        ],
                        Vec::new(),
                    );
                    Ok(DartParameter::new(
                        name,
                        TypeFragment::new(format!("List<{public}>")),
                        argument,
                    ))
                }
                _ => super::super::unsupported("unknown direct-vector parameter element"),
            }
        }
        ParamPlan::ScalarOption { primitive } => {
            let ParameterGroup::ByteSlice(_) = group else {
                return broken("scalar-option Dart parameter disagrees with C bridge group");
            };
            let storage = format!("_l${}Storage", name.as_str());
            let writer = format!("_l${}Writer", name.as_str());
            let public_type = type_name::primitive_type(*primitive)?.optional();
            let argument = DartArgument::new(
                vec![
                    format!(
                        "final {storage} = _$$BoltCallocPtr<$$ffi.Uint8>.alloc(1 + {});",
                        super::super::codec::primitive_size(*primitive)
                    ),
                    format!(
                        "final {writer} = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({storage}.ptr, {storage}.len));"
                    ),
                    format!(
                        "if ({name} == null) {{\n  {writer}.writeU8(0);\n}} else {{\n  {writer}.writeU8(1);\n  {writer}.{}({name});\n}}",
                        super::super::codec::primitive_write_method(*primitive)
                    ),
                ],
                vec![format!("{storage}.ptr"), format!("{writer}.len")],
                Vec::new(),
            );
            Ok(DartParameter::new(name, public_type, argument))
        }
        _ => super::super::unsupported("unknown parameter crossing"),
    }
}

fn render_receiver(
    receiver: &Receiver,
    receive: Receive,
    group: &ParameterGroup,
    function: &impl NativeParameterSource,
    _context: &RenderContext<Native>,
) -> Result<DartArgument> {
    match receiver {
        Receiver::Class => Ok(DartArgument::new(
            vec!["_f$throwIfDisposed();".to_owned()],
            vec!["_handle".to_owned()],
            Vec::new(),
        )),
        Receiver::DirectValue(ty @ DirectValueType::Record(_)) => {
            render_direct_argument("this", ty, receive, group, function)
        }
        Receiver::DirectValue(DirectValueType::Enum(_)) => Ok(DartArgument::new(
            Vec::new(),
            vec!["value".to_owned()],
            Vec::new(),
        )),
        Receiver::DirectValue(DirectValueType::Primitive(_)) => {
            super::super::unsupported("primitive method owner")
        }
        Receiver::DirectValue(_) => super::super::unsupported("unknown direct method owner"),
        Receiver::EncodedValue => {
            let ParameterGroup::ByteSlice(_) = group else {
                return broken("encoded Dart receiver disagrees with C bridge group");
            };
            let storage = "_l$selfStorage";
            let writer = "_l$selfWriter";
            Ok(DartArgument::with_cleanup(
                vec![
                    format!(
                        "final {storage} = _$$BoltStoragePool.acquireStorage(_m$wireEncodedSize());"
                    ),
                    format!(
                        "final {writer} = _$$BoltWireEncoder(_$$BoltBufWriter.fromSpan({storage}.ptr, {storage}.len));"
                    ),
                    format!("_m$wireEncode({writer});"),
                ],
                vec![format!("{storage}.ptr"), format!("{writer}.len")],
                vec![format!("_$$BoltStoragePool.releaseStorage({storage});")],
            ))
        }
    }
}

fn render_direct_argument(
    value: &str,
    ty: &DirectValueType,
    receive: Receive,
    group: &ParameterGroup,
    function: &impl NativeParameterSource,
) -> Result<DartArgument> {
    match ty {
        DirectValueType::Primitive(_) | DirectValueType::Enum(_) => {
            let ParameterGroup::Value(_) = group else {
                return broken("scalar direct Dart parameter disagrees with C bridge group");
            };
            let argument = match ty {
                DirectValueType::Primitive(_) => value.to_owned(),
                DirectValueType::Enum(_) => format!("{value}.value"),
                _ => unreachable!(),
            };
            Ok(DartArgument::new(Vec::new(), vec![argument], Vec::new()))
        }
        DirectValueType::Record(_) => match (receive, group) {
            (Receive::ByValue, ParameterGroup::Value(_)) => Ok(DartArgument::new(
                Vec::new(),
                vec![format!("{value}._m$toStruct()")],
                Vec::new(),
            )),
            (Receive::ByRef, ParameterGroup::Value(index)) => {
                match function.parameter(*index).ty() {
                    CBridgeType::ConstPointer(inner) => {
                        let native = dart_native::NativeType::from_c(inner)?;
                        let storage = format!("_l${}Storage", value.trim_start_matches("this"));
                        Ok(DartArgument::new(
                            vec![
                                format!(
                                    "final {storage} = _$$BoltCallocPtr<{}>.alloc($$ffi.sizeOf<{}>());",
                                    native.native(),
                                    native.native(),
                                ),
                                format!("{value}._m$writeStruct({storage}.ptr);"),
                            ],
                            vec![format!("{storage}.ptr")],
                            Vec::new(),
                        ))
                    }
                    CBridgeType::DirectRecord(_) | CBridgeType::Named(_) => Ok(DartArgument::new(
                        Vec::new(),
                        vec![format!("{value}._m$toStruct()")],
                        Vec::new(),
                    )),
                    _ => broken("borrowed direct record disagrees with its C parameter type"),
                }
            }
            (Receive::ByMutRef, ParameterGroup::DirectWriteback(writeback)) => {
                let output = OutPointer::from_index(writeback.output(), function)?;
                let storage = format!("_l${}Out", value.trim_start_matches("this"));
                Ok(DartArgument::new(
                    vec![output.allocation(&storage)?],
                    vec![format!("{value}._m$toStruct()"), format!("{storage}.ptr")],
                    vec![format!(
                        "{value}._m$updateFromStruct({});",
                        output.read(&format!("{storage}.ptr"))?
                    )],
                ))
            }
            _ => broken("direct record Dart parameter disagrees with C bridge group"),
        },
        _ => super::super::unsupported("unknown direct parameter type"),
    }
}

pub fn render_return(
    plan: &ReturnPlan<Native, boltffi_binding::OutOfRust>,
    error: &ErrorDecl<Native, boltffi_binding::OutOfRust>,
    function: &impl NativeCallableSource,
    groups: &[ParameterGroup],
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<DartReturn> {
    let mut group_index = 0;
    let completion_status = match groups.first() {
        Some(ParameterGroup::CompletionStatusOut(index)) => {
            group_index += 1;
            Some(OutPointer::from_index(*index, function)?)
        }
        _ => None,
    };
    let success_out = match plan {
        ReturnPlan::DirectViaOutPointer { .. }
        | ReturnPlan::EncodedViaOutPointer { .. }
        | ReturnPlan::HandleViaOutPointer { .. } => {
            let group = groups.get(group_index).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart return out-pointer group is missing from the C bridge",
            })?;
            group_index += 1;
            Some(out_pointer(group, function)?)
        }
        _ => None,
    };
    let mut value = match plan {
        ReturnPlan::Void => DartReturn {
            public_type: TypeFragment::new("void"),
            before_call: Vec::new(),
            arguments: Vec::new(),
            call_result: None,
            after_call: Vec::new(),
            expression: None,
        },
        ReturnPlan::DirectViaReturnSlot { ty } => direct_return(ty, None, context)?,
        ReturnPlan::DirectViaOutPointer { ty } => direct_return(ty, success_out, context)?,
        ReturnPlan::EncodedViaReturnSlot { ty, codec, .. } => {
            encoded_return(ty, codec, None, bridge, context)?
        }
        ReturnPlan::EncodedViaOutPointer { ty, codec, .. } => {
            encoded_return(ty, codec, success_out, bridge, context)?
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        } => handle_return(target, *presence, None, context)?,
        ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => handle_return(target, *presence, success_out, context)?,
        ReturnPlan::ScalarOptionViaReturnSlot {
            primitive,
            enum_target,
        } => scalar_option_return(*primitive, enum_target.as_ref(), bridge, context)?,
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            direct_vector_return(element, bridge, context)?
        }
        ReturnPlan::ClosureViaOutPointer(closure) => {
            let protocol = match groups.get(group_index) {
                Some(ParameterGroup::ClosureReturn(protocol)) => protocol,
                _ => return broken("Dart closure return disagrees with C bridge group"),
            };
            group_index += 1;
            let returned = ReturnedClosure::from_declaration(closure, protocol, bridge, context)?;
            DartReturn {
                public_type: returned.public_type,
                before_call: returned.before_call,
                arguments: returned.arguments,
                call_result: None,
                after_call: returned.after_call,
                expression: Some(returned.expression),
            }
        }
        _ => return super::super::unsupported("unknown return crossing"),
    };

    let mut error_checks = Vec::new();
    if let Some(completion_status) = completion_status {
        // Every async completion pays for this allocation, so it skips the
        // finalizer (`allocation_unmanaged`) and disposes synchronously
        // right here instead of relying on eventual GC-driven cleanup.
        value
            .before_call
            .push(completion_status.allocation_unmanaged("_l$completionStatus")?);
        value
            .arguments
            .insert(0, "_l$completionStatus.ptr".to_owned());
        error_checks.push(
            "final _l$completionStatusCode = _l$completionStatus.ptr.ref.code;".to_owned(),
        );
        error_checks.push("_l$completionStatus.dispose();".to_owned());
        error_checks.push(status_check("_l$completionStatusCode"));
    }
    match error {
        ErrorDecl::None(_) => {
            if function.returns() == &CBridgeType::Status {
                value.call_result = Some("_l$status".to_owned());
                error_checks.push(status_check("_l$status.code"));
            }
        }
        ErrorDecl::StatusViaReturnSlot { .. } => {
            value.call_result = Some("_l$status".to_owned());
            error_checks.push(status_check("_l$status"));
        }
        ErrorDecl::StatusViaOutPointer { .. } => {
            let out = groups.get(group_index).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart status error out-pointer is missing from the C bridge",
            })?;
            group_index += 1;
            let out = out_pointer(out, function)?;
            value.before_call.push(out.allocation("_l$errorOut")?);
            value.arguments.push("_l$errorOut.ptr".to_owned());
            error_checks.push(status_check("_l$errorOut.ptr.ref.code"));
        }
        ErrorDecl::EncodedViaReturnSlot { ty, codec, .. } => {
            value.call_result = Some("_l$error".to_owned());
            error_checks.extend(encoded_error_check("_l$error", ty, codec, bridge, context)?);
        }
        ErrorDecl::EncodedViaOutPointer { ty, codec, .. } => {
            let out = groups.get(group_index).ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "Dart encoded error out-pointer is missing from the C bridge",
            })?;
            group_index += 1;
            let out = out_pointer(out, function)?;
            value.before_call.push(out.allocation("_l$errorOut")?);
            value.arguments.push("_l$errorOut.ptr".to_owned());
            error_checks.push("final _l$error = _l$errorOut.ptr.ref;".to_owned());
            error_checks.extend(encoded_error_check("_l$error", ty, codec, bridge, context)?);
        }
        _ => return super::super::unsupported("unknown error declaration"),
    }
    if group_index != groups.len() {
        return broken("Dart callable left unconsumed C return parameter groups");
    }
    value.after_call.splice(0..0, error_checks);
    Ok(value)
}

fn status_check(status: &str) -> String {
    format!(
        "if ({status} != 0) {{ throw $$BoltException('BoltFFI call failed with status ${{{status}}}'); }}"
    )
}

fn encoded_error_check(
    buffer: &str,
    ty: &TypeRef,
    codec: &ReadPlan,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Vec<String>> {
    let read = codec
        .render_with(&mut Reader::new("_l$errorReader", context))?
        .into_source();
    let error = match ty {
        TypeRef::String => format!("$$BoltException({read})"),
        TypeRef::Record(_) | TypeRef::Enum(_) => read,
        _ => return super::super::unsupported("Dart encoded error payload"),
    };
    Ok(vec![
        format!("if ({buffer}.ptr != $$ffi.nullptr) {{"),
        "  try {".to_owned(),
        format!(
            "    final _l$errorReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan({buffer}.ptr, {buffer}.len));"
        ),
        format!("    throw {error};"),
        "  } finally {".to_owned(),
        format!(
            "    _f${}({buffer});",
            bridge.support().buffer_free()?.name()
        ),
        "  }".to_owned(),
        "}".to_owned(),
    ])
}

fn direct_return(
    ty: &DirectValueType,
    out: Option<OutPointer>,
    context: &RenderContext<Native>,
) -> Result<DartReturn> {
    let expression = match ty {
        DirectValueType::Primitive(_) => "_l$result".to_owned(),
        DirectValueType::Enum(_) => format!(
            "{}._m$fromDiscriminant(_l$result)",
            type_name::direct_value(ty, context)?
        ),
        DirectValueType::Record(_) => format!(
            "{}._m$fromStruct(_l$result)",
            type_name::direct_value(ty, context)?
        ),
        _ => return super::super::unsupported("unknown direct return type"),
    };
    out_return(type_name::direct_value(ty, context)?, expression, out)
}

fn encoded_return(
    ty: &TypeRef,
    codec: &ReadPlan,
    out: Option<OutPointer>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<DartReturn> {
    let expression = codec
        .render_with(&mut Reader::new("_l$resultReader", context))?
        .into_source();
    let mut value = out_return(type_name::type_ref(ty, context)?, expression, out)?;
    // A plain `late final` + nested try/finally instead of an
    // immediately-invoked closure: same result, no closure allocation.
    value.after_call.push(format!(
        "late final {} _l$decodedResult;\ntry {{\n  final _l$resultReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan(_l$result.ptr, _l$result.len));\n  _l$decodedResult = {};\n}} finally {{\n  _f${}(_l$result);\n}}",
        value.public_type.as_str(),
        value.expression.as_deref().unwrap_or("null"),
        bridge.support().buffer_free()?.name(),
    ));
    value.expression = Some("_l$decodedResult".to_owned());
    Ok(value)
}

fn handle_return(
    target: &HandleTarget,
    presence: HandlePresence,
    out: Option<OutPointer>,
    context: &RenderContext<Native>,
) -> Result<DartReturn> {
    let ty = type_name::handle(target, presence, context)?;
    let required = type_name::handle(target, HandlePresence::Required, context)?;
    let expression = match (target, presence) {
        (HandleTarget::Class(_), HandlePresence::Required) => {
            format!("{required}._(_l$result)")
        }
        (HandleTarget::Class(_), HandlePresence::Nullable) => {
            format!("_l$result == 0 ? null : {required}._(_l$result)")
        }
        (HandleTarget::Callback(_), HandlePresence::Required) => {
            format!("{required}Bridge.wrap(_l$result)")
        }
        (HandleTarget::Callback(_), HandlePresence::Nullable) => {
            format!("_l$result.handle == 0 ? null : {required}Bridge.wrap(_l$result)")
        }
        (HandleTarget::Stream(_), _) => {
            return super::super::unsupported("stream handle return");
        }
        _ => return super::super::unsupported("unknown handle presence"),
    };
    out_return(ty, expression, out)
}

fn scalar_option_return(
    primitive: boltffi_binding::Primitive,
    enum_target: Option<&TypeRef>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<DartReturn> {
    let public_inner = enum_target.map_or_else(
        || type_name::primitive_type(primitive),
        |target| type_name::type_ref(target, context),
    )?;
    let decoded_type = type_name::primitive_type(primitive)?.optional();
    let decoded = format!(
        "_l$resultReader.readU8() == 0 ? null : _l$resultReader.{}()",
        super::super::codec::primitive_read_method(primitive)
    );
    let expression = match enum_target {
        Some(_) => {
            format!("_l$decoded == null ? null : {public_inner}._m$fromDiscriminant(_l$decoded)")
        }
        None => "_l$decoded".to_owned(),
    };
    Ok(DartReturn {
        public_type: public_inner.optional(),
        before_call: Vec::new(),
        arguments: Vec::new(),
        call_result: Some("_l$result".to_owned()),
        after_call: vec![format!(
            "late final {} _l$decoded;\ntry {{\n  final _l$resultReader = _$$BoltWireDecoder(_$$BoltBufReader.fromSpan(_l$result.ptr, _l$result.len));\n  _l$decoded = {decoded};\n}} finally {{\n  _f${}(_l$result);\n}}",
            decoded_type.as_str(),
            bridge.support().buffer_free()?.name(),
        )],
        expression: Some(expression),
    })
}

fn direct_vector_return(
    element: &DirectVectorElementType,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<DartReturn> {
    let (setup, expression) = match element {
        DirectVectorElementType::Primitive(primitive) => {
            let primitive = primitive.primitive();
            let vector = PrimitiveVector::new(primitive)?;
            let count = format!(
                "_l$result.len ~/ $$ffi.sizeOf<{}>()",
                vector.native().native()
            );
            let expression = vector.copied_from("_l$result.ptr", &count)?;
            (Vec::new(), expression)
        }
        DirectVectorElementType::Record(record) => {
            let public = type_name::direct_value(&DirectValueType::Record(*record), context)?;
            let native = dart_native::direct_record_struct(bridge, *record)?;
            (
                vec![format!(
                    "final _l$count = _l$result.len ~/ $$ffi.sizeOf<{native}>();"
                )],
                format!(
                    "List<{public}>.generate(_l$count, (_l$index) => {public}._m$fromStruct(_l$result.ptr.cast<{native}>().elementAt(_l$index).ref))"
                ),
            )
        }
        _ => return super::super::unsupported("unknown direct-vector return element"),
    };
    let public_type = type_name::direct_vector(element, context)?;
    let mut after_call = setup;
    after_call.push(format!(
        "late final {} _l$decoded;\ntry {{\n  _l$decoded = {expression};\n}} finally {{\n  _f${}(_l$result);\n}}",
        public_type.as_str(),
        bridge.support().buffer_free()?.name(),
    ));
    Ok(DartReturn {
        public_type,
        before_call: Vec::new(),
        arguments: Vec::new(),
        call_result: Some("_l$result".to_owned()),
        after_call,
        expression: Some("_l$decoded".to_owned()),
    })
}

fn out_return(
    public_type: TypeFragment,
    expression: String,
    out: Option<OutPointer>,
) -> Result<DartReturn> {
    match out {
        None => Ok(DartReturn {
            public_type,
            before_call: Vec::new(),
            arguments: Vec::new(),
            call_result: Some("_l$result".to_owned()),
            after_call: Vec::new(),
            expression: Some(expression),
        }),
        Some(out) => Ok(DartReturn {
            public_type,
            before_call: vec![out.allocation("_l$resultOut")?],
            arguments: vec!["_l$resultOut.ptr".to_owned()],
            call_result: None,
            after_call: vec![format!(
                "final _l$result = {};",
                out.read("_l$resultOut.ptr")?
            )],
            expression: Some(expression),
        }),
    }
}

fn render_sync_call(
    function: &CFunction,
    arguments: &[String],
    setup: &[String],
    cleanup: &[String],
    returns: &DartReturn,
) -> String {
    let mut statements = setup.to_vec();
    statements.extend(returns.before_call.iter().cloned());
    let mut arguments = arguments.to_vec();
    arguments.extend(returns.arguments.iter().cloned());
    let invocation = format!("_f${}({})", function.name(), arguments.join(", "));
    statements.push(match &returns.call_result {
        Some(result) => format!("final {result} = {invocation};"),
        None => format!("{invocation};"),
    });
    statements.extend(returns.after_call.iter().cloned());
    statements.extend(cleanup.iter().cloned());
    if let Some(expression) = &returns.expression {
        statements.push(format!("return {expression};"));
    }
    statements.join("\n")
}

fn render_async_call(
    start: &CFunction,
    asynchronous: AsyncFunctions<'_>,
    arguments: &[String],
    setup: &[String],
    cleanup: &[String],
    returns: &DartReturn,
) -> Result<String> {
    let create_body = {
        let mut statements = setup.to_vec();
        statements.push(format!(
            "final _l$future = _f${}({});",
            start.name(),
            arguments.join(", ")
        ));
        statements.extend(cleanup.iter().cloned());
        statements.push("return _l$future;".to_owned());
        statements.join("\n")
    };
    let completion_body = {
        let mut completion_arguments = vec!["_p$handle".to_owned()];
        completion_arguments.extend(returns.arguments.iter().cloned());
        let invocation = format!(
            "_f${}({})",
            asynchronous.completion.name(),
            completion_arguments.join(", ")
        );
        let mut statements = returns.before_call.clone();
        statements.push(match &returns.call_result {
            Some(result) => format!("final {result} = {invocation};"),
            None => format!("{invocation};"),
        });
        statements.extend(returns.after_call.iter().cloned());
        if let Some(expression) = &returns.expression {
            statements.push(format!("return {expression};"));
        }
        statements.join("\n")
    };
    Ok(format!(
        "return _$$BoltFFIAsync.create(\n  createFuture: () {{\n{}\n  }},\n  pollFuture: _f${},\n  completeFuture: (_p$handle) {{\n{}\n  }},\n  freeFuture: _f${},\n  cancelFuture: _f${},\n);",
        indent(&create_body, 4),
        asynchronous.poll.name().as_str(),
        indent(&completion_body, 4),
        asynchronous.free.name().as_str(),
        asynchronous.cancel.name().as_str(),
    ))
}

fn factory_constructor_name(name: &Identifier) -> Option<Identifier> {
    match name.as_str() {
        "new" | "$new" => None,
        _ => Some(name.clone()),
    }
}

fn out_pointer(
    group: &ParameterGroup,
    function: &impl NativeParameterSource,
) -> Result<OutPointer> {
    let index = match group {
        ParameterGroup::Value(index) | ParameterGroup::SuccessOut(index) => *index,
        _ => return broken("Dart return storage is not a C out-pointer group"),
    };
    OutPointer::from_index(index, function)
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
