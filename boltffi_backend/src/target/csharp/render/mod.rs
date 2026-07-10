mod enumeration;
mod record;

pub(in crate::target::csharp) use enumeration::Enumeration;
pub(in crate::target::csharp) use record::Record;

use std::collections::BTreeMap;

use askama::Template;
use boltffi_binding::{
    CanonicalName, DeclarationRef, DirectValueType, DirectVectorElementType, ErrorChannel,
    ErrorPlacement, ExecutionDecl, ExportedCallable, ExportedMethodDecl, FunctionDecl,
    IncomingParam, InitializerDecl, Native, NativeSymbol, OutOfRust, ParamPlan, Primitive,
    ReadPlan, Receive, ReturnPlan, TypeRef, WritePlan, native,
};

use crate::{
    bridge::c::{
        CBridgeContract, Function as CFunction, ParameterGroup, ReturnChannel, Type as CBridgeType,
    },
    core::{
        AuxChunk, Diagnostic, Emitted, Error, FilePath, GeneratedFile, GeneratedOutput, HelperId,
        RenderContext, RenderedDeclaration, Result,
    },
};

use super::{
    codec::{ReadExpression, Reader, Writer, primitive_read_method, primitive_write_method},
    name_style::{Name, Namespace},
    syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeFragment},
    type_name,
};

const TARGET: &str = "csharp";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Parameter {
    name: Identifier,
    ty: TypeFragment,
    marshal_i1: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeParameter {
    name: Identifier,
    ty: TypeFragment,
    modifier: &'static str,
    marshal_i1: bool,
    marshal_bool_array: bool,
    byte_array: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CallSite {
    Free,
    Record {
        owner: DirectValueType,
        name: Identifier,
    },
    Enumeration {
        owner: DirectValueType,
        name: Identifier,
    },
}

#[derive(Clone, Copy)]
struct EncodedReceiverPlan<'plan> {
    read: &'plan ReadPlan,
    write: &'plan WritePlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Function {
    name: Identifier,
    native_name: Identifier,
    parameters: Vec<Parameter>,
    native_parameters: Vec<NativeParameter>,
    public_return_type: TypeFragment,
    native_return_type: TypeFragment,
    return_marshal_i1: bool,
    checks_status: bool,
    is_static: bool,
    extension_owner: Option<TypeFragment>,
    return_after_status: Option<Expression>,
    body: Option<Statement>,
    requires_wire_runtime: bool,
    free_buffer_entry: Option<Literal>,
    invocation: Expression,
    entry_point: Literal,
    helper_id: HelperId,
}

#[derive(Template)]
#[template(path = "target/csharp/function.cs", escape = "none")]
struct FunctionTemplate<'function> {
    function: &'function Function,
}

#[derive(Template)]
#[template(path = "target/csharp/native_function.cs", escape = "none")]
struct NativeFunctionTemplate<'function> {
    function: &'function Function,
}

#[derive(Template)]
#[template(path = "target/csharp/status.cs", escape = "none")]
struct StatusTemplate;

#[derive(Template)]
#[template(path = "target/csharp/wire.cs", escape = "none")]
struct WireTemplate;

#[derive(Template)]
#[template(path = "target/csharp/free_buffer.cs", escape = "none")]
struct FreeBufferTemplate<'entry> {
    entry_point: &'entry Literal,
}

#[derive(Template)]
#[template(path = "target/csharp/module.cs", escape = "none")]
struct ModuleTemplate<'module> {
    namespace: &'module Namespace,
    class_name: &'module Identifier,
    library: &'module Literal,
    support: &'module [Statement],
    functions: &'module [Statement],
    native_functions: &'module [Statement],
}

impl Function {
    pub(super) fn from_declaration(
        declaration: &FunctionDecl<Native>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).pascal()?;
        Self::from_callable(
            name.clone(),
            Identifier::parse(format!("Native{name}"))?,
            HelperId::new(declaration.name().clone()),
            declaration.symbol(),
            declaration.callable(),
            CallSite::Free,
            None,
            None,
            bridge,
            context,
        )
    }

    pub(super) fn from_initializer(
        declaration: &InitializerDecl<Native>,
        owner: DirectValueType,
        owner_name: &Identifier,
        extension: bool,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_associated(
            declaration.name(),
            declaration.symbol(),
            declaration.callable(),
            owner,
            owner_name,
            extension,
            None,
            None,
            bridge,
            context,
        )
    }

    pub(super) fn from_initializer_qualified(
        declaration: &InitializerDecl<Native>,
        owner: DirectValueType,
        owner_name: &Identifier,
        namespace: &Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_associated(
            declaration.name(),
            declaration.symbol(),
            declaration.callable(),
            owner,
            owner_name,
            false,
            Some(namespace),
            None,
            bridge,
            context,
        )
    }

    pub(super) fn from_method(
        declaration: &ExportedMethodDecl<Native, NativeSymbol>,
        owner: DirectValueType,
        owner_name: &Identifier,
        extension: bool,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_associated(
            declaration.name(),
            declaration.target(),
            declaration.callable(),
            owner,
            owner_name,
            extension,
            None,
            None,
            bridge,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_encoded_method(
        declaration: &ExportedMethodDecl<Native, NativeSymbol>,
        owner: DirectValueType,
        owner_name: &Identifier,
        read: &ReadPlan,
        write: &WritePlan,
        type_namespace: Option<&Namespace>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_associated(
            declaration.name(),
            declaration.target(),
            declaration.callable(),
            owner,
            owner_name,
            false,
            type_namespace,
            Some(EncodedReceiverPlan { read, write }),
            bridge,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_associated(
        declaration_name: &CanonicalName,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        owner: DirectValueType,
        owner_name: &Identifier,
        extension: bool,
        type_namespace: Option<&Namespace>,
        encoded_receiver: Option<EncodedReceiverPlan<'_>>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration_name).pascal()?;
        let call_site = match extension {
            true => CallSite::Enumeration {
                owner,
                name: owner_name.clone(),
            },
            false => CallSite::Record {
                owner,
                name: owner_name.clone(),
            },
        };
        Self::from_callable(
            name.clone(),
            Identifier::parse(format!("Native{owner_name}{name}"))?,
            HelperId::new(CanonicalName::single(symbol.name().as_str())),
            symbol,
            callable,
            call_site,
            type_namespace,
            encoded_receiver,
            bridge,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_callable(
        name: Identifier,
        native_name: Identifier,
        helper_id: HelperId,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        call_site: CallSite,
        type_namespace: Option<&Namespace>,
        encoded_receiver: Option<EncodedReceiverPlan<'_>>,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_function = bridge_function(symbol, bridge)?;

        if !matches!(callable.execution(), ExecutionDecl::Synchronous(_)) {
            return unsupported("asynchronous functions");
        }
        let mut parameter_groups = c_function.parameter_groups().iter();
        let mut native_parameters = Vec::new();
        let mut invocation_arguments = Vec::new();
        let mut return_after_status = None;
        let mut encoded_writeback = None;
        let mut setup = Vec::new();
        let mut requires_wire_runtime = false;
        let encoded_error = lower_error(
            callable.error().channel(),
            type_namespace,
            c_function,
            context,
        )?;
        requires_wire_runtime |= encoded_error.is_some();

        if let Some(receive) = callable.receiver() {
            let (owner, owner_name, extension) = match &call_site {
                CallSite::Record { owner, name } => (owner, name, false),
                CallSite::Enumeration { owner, name } => (owner, name, true),
                CallSite::Free => return unsupported("free function receiver"),
            };
            let group = parameter_groups.next().ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "method receiver is missing from the C bridge",
            })?;
            let receiver = match encoded_receiver {
                Some(receiver) => lower_encoded_receiver(
                    receive,
                    group,
                    receiver,
                    type_namespace,
                    c_function,
                    context,
                )?,
                None => lower_receiver(
                    owner, owner_name, receive, extension, group, c_function, bridge,
                )?,
            };
            native_parameters.extend(receiver.native_parameters);
            invocation_arguments.extend(receiver.arguments);
            return_after_status = receiver.return_after_status;
            encoded_writeback = receiver.encoded_writeback;
            setup.extend(receiver.setup);
            requires_wire_runtime |= receiver.requires_wire_runtime;
        }

        let mut parameters = Vec::new();
        for parameter in callable.params() {
            let group = parameter_groups.next().ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "function parameter is missing from the C bridge",
            })?;
            let name = Name::new(parameter.name()).camel()?;
            match parameter.payload() {
                IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                    let ParameterGroup::Value(index) = group else {
                        return unsupported("mutable direct function parameters");
                    };
                    let c_parameter = c_function.parameter(*index);
                    let modifier =
                        direct_parameter_modifier(ty, *receive, c_parameter.ty(), bridge)?;
                    let rendered_type = direct_type_with(ty, type_namespace, context)?;
                    let marshal_i1 = matches!(ty, DirectValueType::Primitive(Primitive::Bool));
                    parameters.push(Parameter {
                        name: name.clone(),
                        ty: rendered_type.clone(),
                        marshal_i1,
                    });
                    native_parameters.push(NativeParameter {
                        name: name.clone(),
                        ty: rendered_type,
                        modifier,
                        marshal_i1,
                        marshal_bool_array: false,
                        byte_array: false,
                    });
                    invocation_arguments.push(Expression::identifier(name));
                }
                IncomingParam::Value(ParamPlan::Encoded {
                    ty,
                    codec,
                    shape,
                    receive,
                }) => {
                    if *shape != native::BufferShape::Slice {
                        return unsupported("encoded function parameter shape");
                    }
                    if matches!(receive, Receive::ByMutRef) {
                        return unsupported("mutable encoded function parameters");
                    }
                    let ParameterGroup::ByteSlice(slice) = group else {
                        return broken_contract(
                            "encoded function parameter does not use a C byte slice",
                        );
                    };
                    if !matches!(
                        c_function.parameter(slice.pointer()).ty(),
                        CBridgeType::ConstPointer(inner) if inner.as_ref() == &CBridgeType::Uint8
                    ) || c_function.parameter(slice.length()).ty() != &CBridgeType::PointerWidth
                    {
                        return broken_contract(
                            "encoded function parameter does not match the C bridge",
                        );
                    }
                    parameters.push(Parameter {
                        name: name.clone(),
                        ty: render_type_ref(ty, type_namespace, context)?,
                        marshal_i1: false,
                    });
                    let writer = generated_identifier(&name, "Writer")?;
                    let bytes = generated_identifier(&name, "Bytes")?;
                    let writes = codec
                        .render_with(&mut Writer::new(
                            writer.clone(),
                            Expression::identifier(name.clone()),
                            context,
                        ))
                        .into_iter()
                        .collect::<Result<Vec<_>>>()?;
                    setup.push(Statement::new(format!(
                        "WireWriter {writer} = new WireWriter();"
                    )));
                    setup.extend(writes);
                    setup.push(Statement::new(format!(
                        "byte[] {bytes} = {writer}.ToArray();"
                    )));
                    native_parameters.extend([
                        NativeParameter {
                            name: bytes.clone(),
                            ty: TypeFragment::new("byte[]"),
                            modifier: "",
                            marshal_i1: false,
                            marshal_bool_array: false,
                            byte_array: true,
                        },
                        NativeParameter {
                            name: generated_identifier(&name, "Length")?,
                            ty: TypeFragment::new("nuint"),
                            modifier: "",
                            marshal_i1: false,
                            marshal_bool_array: false,
                            byte_array: false,
                        },
                    ]);
                    invocation_arguments.extend([
                        Expression::identifier(bytes.clone()),
                        Expression::new(format!("(nuint){bytes}.Length")),
                    ]);
                    requires_wire_runtime = true;
                }
                IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                    let ParameterGroup::ByteSlice(slice) = group else {
                        return broken_contract(
                            "scalar option parameter does not use a C byte slice",
                        );
                    };
                    if !matches!(
                        c_function.parameter(slice.pointer()).ty(),
                        CBridgeType::ConstPointer(inner) if inner.as_ref() == &CBridgeType::Uint8
                    ) || c_function.parameter(slice.length()).ty() != &CBridgeType::PointerWidth
                    {
                        return broken_contract(
                            "scalar option parameter does not match the C bridge",
                        );
                    }
                    parameters.push(Parameter {
                        name: name.clone(),
                        ty: TypeFragment::new(format!("{}?", primitive_type(*primitive))),
                        marshal_i1: false,
                    });
                    let writer = generated_identifier(&name, "Writer")?;
                    let bytes = generated_identifier(&name, "Bytes")?;
                    setup.push(Statement::new(format!(
                        "WireWriter {writer} = new WireWriter();"
                    )));
                    setup.push(Statement::new(format!(
                        "if ({name}.HasValue)\n{{\n    {writer}.WriteU8(1);\n    {writer}.{}({name}.Value);\n}}\nelse\n{{\n    {writer}.WriteU8(0);\n}}",
                        primitive_write_method(*primitive)
                    )));
                    setup.push(Statement::new(format!(
                        "byte[] {bytes} = {writer}.ToArray();"
                    )));
                    native_parameters.extend([
                        NativeParameter {
                            name: bytes.clone(),
                            ty: TypeFragment::new("byte[]"),
                            modifier: "",
                            marshal_i1: false,
                            marshal_bool_array: false,
                            byte_array: true,
                        },
                        NativeParameter {
                            name: generated_identifier(&name, "Length")?,
                            ty: TypeFragment::new("nuint"),
                            modifier: "",
                            marshal_i1: false,
                            marshal_bool_array: false,
                            byte_array: false,
                        },
                    ]);
                    invocation_arguments.extend([
                        Expression::identifier(bytes.clone()),
                        Expression::new(format!("(nuint){bytes}.Length")),
                    ]);
                    requires_wire_runtime = true;
                }
                IncomingParam::Value(ParamPlan::DirectVec { element }) => {
                    let ParameterGroup::DirectVector(vector) = group else {
                        return broken_contract(
                            "direct-vector parameter does not use a C vector group",
                        );
                    };
                    if c_function.parameter(vector.length()).ty() != &CBridgeType::PointerWidth {
                        return broken_contract(
                            "direct-vector parameter length does not match the C bridge",
                        );
                    }
                    let pointer_matches =
                        match (element, c_function.parameter(vector.pointer()).ty()) {
                            (
                                DirectVectorElementType::Primitive(primitive),
                                CBridgeType::ConstPointer(inner),
                            ) => inner.as_ref() == &CBridgeType::primitive(primitive.primitive())?,
                            (
                                DirectVectorElementType::Record(_),
                                CBridgeType::ConstPointer(inner),
                            ) => inner.as_ref() == &CBridgeType::Uint8,
                            _ => false,
                        };
                    if !pointer_matches {
                        return broken_contract(
                            "direct-vector parameter pointer does not match the C bridge",
                        );
                    }
                    let element_type =
                        direct_vector_element_type(element, type_namespace, context)?;
                    let array_type = TypeFragment::new(format!("{element_type}[]"));
                    parameters.push(Parameter {
                        name: name.clone(),
                        ty: array_type.clone(),
                        marshal_i1: false,
                    });
                    native_parameters.extend([
                        NativeParameter {
                            name: name.clone(),
                            ty: array_type,
                            modifier: "",
                            marshal_i1: false,
                            marshal_bool_array: matches!(
                                element,
                                DirectVectorElementType::Primitive(primitive)
                                    if primitive.primitive() == Primitive::Bool
                            ),
                            byte_array: true,
                        },
                        NativeParameter {
                            name: generated_identifier(&name, "Length")?,
                            ty: TypeFragment::new("nuint"),
                            modifier: "",
                            marshal_i1: false,
                            marshal_bool_array: false,
                            byte_array: false,
                        },
                    ]);
                    invocation_arguments.push(Expression::identifier(name.clone()));
                    invocation_arguments.push(match element {
                        DirectVectorElementType::Primitive(_) => {
                            Expression::new(format!("(nuint){name}.Length"))
                        }
                        DirectVectorElementType::Record(_) => Expression::new(format!(
                            "(nuint)({name}.Length * Marshal.SizeOf<{element_type}>())"
                        )),
                        _ => return unsupported("direct-vector element type"),
                    });
                }
                _ => return unsupported("function parameter crossing"),
            }
        }

        let return_plan = callable.returns().plan();
        if (return_after_status.is_some() || encoded_writeback.is_some())
            && !matches!(return_plan, ReturnPlan::Void)
        {
            return unsupported("mutable value method returns");
        }
        let mut encoded_return = None;
        let (public_return_type, native_return_type, return_marshal_i1, checks_status) =
            match return_plan {
                ReturnPlan::Void => {
                    let expected_return = if encoded_error.is_some() {
                        CBridgeType::Buffer
                    } else {
                        CBridgeType::Status
                    };
                    if c_function.returns() != &expected_return {
                        return broken_contract("void return type does not match the C bridge");
                    }
                    let public_return_type = match (
                        return_after_status.is_some() || encoded_writeback.is_some(),
                        &call_site,
                    ) {
                        (true, CallSite::Record { owner, .. }) => {
                            direct_type_with(owner, type_namespace, context)?
                        }
                        (true, _) => return unsupported("mutable enum receiver"),
                        (false, _) => TypeFragment::void(),
                    };
                    (
                        public_return_type,
                        if encoded_error.is_some() {
                            TypeFragment::new("FfiBuf")
                        } else {
                            TypeFragment::new("FfiStatus")
                        },
                        false,
                        encoded_error.is_none(),
                    )
                }
                ReturnPlan::DirectViaReturnSlot { ty } => {
                    if encoded_error.is_some() {
                        return broken_contract(
                            "fallible direct return does not use a success out-pointer",
                        );
                    }
                    if return_after_status.is_some() || encoded_writeback.is_some() {
                        return unsupported("mutable value method returns");
                    }
                    if !c_direct_matches(ty, c_function.returns(), bridge)? {
                        return broken_contract("function return type does not match the C bridge");
                    }
                    let rendered = direct_type_with(ty, type_namespace, context)?;
                    (
                        rendered.clone(),
                        rendered,
                        matches!(ty, DirectValueType::Primitive(Primitive::Bool)),
                        false,
                    )
                }
                ReturnPlan::EncodedViaReturnSlot { ty, codec, shape } => {
                    if encoded_error.is_some() {
                        return broken_contract(
                            "fallible encoded return does not use a success out-pointer",
                        );
                    }
                    if *shape != native::BufferShape::Buffer
                        || c_function.returns() != &CBridgeType::Buffer
                    {
                        return unsupported("encoded function return shape");
                    }
                    let reader = Identifier::parse("resultReader")?;
                    let mut codec_reader = Reader::new(reader.clone(), context);
                    if let Some(namespace) = type_namespace {
                        codec_reader = codec_reader.qualified(namespace);
                    }
                    let decode = codec
                        .render_with(&mut codec_reader)
                        .map(ReadExpression::into_expression)?;
                    encoded_return = Some(EncodedReturn {
                        buffer: Identifier::parse("boltffiResultBuffer")?,
                        reader,
                        decode,
                    });
                    requires_wire_runtime = true;
                    (
                        render_type_ref(ty, type_namespace, context)?,
                        TypeFragment::new("FfiBuf"),
                        false,
                        false,
                    )
                }
                ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                    if encoded_error.is_some() {
                        return broken_contract(
                            "fallible scalar-option return does not use a success out-pointer",
                        );
                    }
                    if c_function.returns() != &CBridgeType::Buffer {
                        return broken_contract("scalar option return does not match the C bridge");
                    }
                    let reader = Identifier::parse("resultReader")?;
                    let ty = TypeFragment::new(format!("{}?", primitive_type(*primitive)));
                    let decode = Expression::new(format!(
                        "{reader}.ReadU8() == 0 ? default({ty}) : {reader}.{}()",
                        primitive_read_method(*primitive)
                    ));
                    encoded_return = Some(EncodedReturn {
                        buffer: Identifier::parse("boltffiResultBuffer")?,
                        reader,
                        decode,
                    });
                    requires_wire_runtime = true;
                    (ty, TypeFragment::new("FfiBuf"), false, false)
                }
                ReturnPlan::DirectVecViaReturnSlot { element } => {
                    if encoded_error.is_some() {
                        return broken_contract(
                            "fallible direct-vector return does not use a success out-pointer",
                        );
                    }
                    if c_function.returns() != &CBridgeType::Buffer {
                        return broken_contract("direct-vector return does not match the C bridge");
                    }
                    let element_type =
                        direct_vector_element_type(element, type_namespace, context)?;
                    let reader = Identifier::parse("resultReader")?;
                    let decode = match element {
                        DirectVectorElementType::Primitive(primitive)
                            if primitive.primitive() == Primitive::Bool =>
                        {
                            Expression::new(format!("{reader}.ReadRawBoolArray()"))
                        }
                        DirectVectorElementType::Primitive(_)
                        | DirectVectorElementType::Record(_) => {
                            Expression::new(format!("{reader}.ReadRawArray<{element_type}>()"))
                        }
                        _ => return unsupported("direct-vector return element type"),
                    };
                    encoded_return = Some(EncodedReturn {
                        buffer: Identifier::parse("boltffiResultBuffer")?,
                        reader,
                        decode,
                    });
                    requires_wire_runtime = true;
                    (
                        TypeFragment::new(format!("{element_type}[]")),
                        TypeFragment::new("FfiBuf"),
                        false,
                        false,
                    )
                }
                ReturnPlan::DirectViaOutPointer { ty } => {
                    if encoded_error.is_none() {
                        return broken_contract(
                            "direct success out-pointer is missing an error channel",
                        );
                    }
                    let Some(ParameterGroup::SuccessOut(index)) = parameter_groups.next() else {
                        return broken_contract(
                            "direct success return is missing its C out-pointer",
                        );
                    };
                    let matches = match c_function.parameter(*index).ty() {
                        CBridgeType::MutPointer(inner) => c_direct_matches(ty, inner, bridge)?,
                        _ => false,
                    };
                    if !matches {
                        return broken_contract(
                            "direct success out-pointer does not match the C bridge",
                        );
                    }
                    let rendered = direct_type_with(ty, type_namespace, context)?;
                    let result = Identifier::parse("boltffiResult")?;
                    native_parameters.push(NativeParameter {
                        name: result.clone(),
                        ty: rendered.clone(),
                        modifier: "out ",
                        marshal_i1: false,
                        marshal_bool_array: false,
                        byte_array: false,
                    });
                    invocation_arguments.push(Expression::new(format!("out {rendered} {result}")));
                    return_after_status = Some(Expression::identifier(result));
                    (rendered, TypeFragment::new("FfiBuf"), false, false)
                }
                ReturnPlan::EncodedViaOutPointer { ty, codec, shape } => {
                    if encoded_error.is_none() || *shape != native::BufferShape::Buffer {
                        return unsupported("encoded success out-pointer shape");
                    }
                    let Some(ParameterGroup::SuccessOut(index)) = parameter_groups.next() else {
                        return broken_contract(
                            "encoded success return is missing its C out-pointer",
                        );
                    };
                    if c_function.parameter(*index).ty()
                        != &CBridgeType::MutPointer(Box::new(CBridgeType::Buffer))
                    {
                        return broken_contract(
                            "encoded success out-pointer does not match the C bridge",
                        );
                    }
                    let buffer = Identifier::parse("boltffiResultBuffer")?;
                    native_parameters.push(NativeParameter {
                        name: buffer.clone(),
                        ty: TypeFragment::new("FfiBuf"),
                        modifier: "out ",
                        marshal_i1: false,
                        marshal_bool_array: false,
                        byte_array: false,
                    });
                    invocation_arguments.push(Expression::new(format!("out FfiBuf {buffer}")));
                    let reader = Identifier::parse("resultReader")?;
                    let mut codec_reader = Reader::new(reader.clone(), context);
                    if let Some(namespace) = type_namespace {
                        codec_reader = codec_reader.qualified(namespace);
                    }
                    let decode = codec
                        .render_with(&mut codec_reader)
                        .map(ReadExpression::into_expression)?;
                    encoded_return = Some(EncodedReturn {
                        buffer,
                        reader,
                        decode,
                    });
                    requires_wire_runtime = true;
                    (
                        render_type_ref(ty, type_namespace, context)?,
                        TypeFragment::new("FfiBuf"),
                        false,
                        false,
                    )
                }
                _ => return unsupported("non-primitive function returns"),
            };

        if parameter_groups.next().is_some() {
            return broken_contract("function parameter group count does not match the C bridge");
        }

        let invocation = Expression::call(
            Expression::member(Identifier::parse("NativeMethods")?, native_name.clone()),
            ArgumentList::new(invocation_arguments),
        );
        let receiver = callable.receiver().is_some();
        let extension_owner = match (&call_site, receiver) {
            (CallSite::Enumeration { owner, .. }, true) => Some(direct_type(owner, context)?),
            _ => None,
        };
        let is_static = !receiver || extension_owner.is_some();
        let body = (!setup.is_empty()
            || encoded_return.is_some()
            || encoded_writeback.is_some()
            || encoded_error.is_some())
        .then(|| {
            render_callable_body(
                &setup,
                &invocation,
                checks_status,
                return_after_status.as_ref(),
                encoded_return.as_ref(),
                encoded_writeback.as_ref(),
                encoded_error.as_ref(),
            )
        })
        .transpose()?;
        let free_buffer_entry = encoded_return
            .as_ref()
            .or(encoded_writeback.as_ref())
            .map(|value| &value.buffer)
            .or_else(|| encoded_error.as_ref().map(|value| &value.buffer))
            .map(|_| {
                bridge
                    .support()
                    .buffer_free()
                    .map(|function| Literal::string(function.name()))
            })
            .transpose()?;

        Ok(Self {
            name,
            native_name,
            parameters,
            native_parameters,
            public_return_type,
            native_return_type,
            return_marshal_i1,
            checks_status,
            is_static,
            extension_owner,
            return_after_status,
            body,
            requires_wire_runtime,
            free_buffer_entry,
            invocation,
            entry_point: Literal::string(c_function.name()),
            helper_id,
        })
    }

    pub(super) fn render(&self) -> Result<Emitted> {
        let emitted = Emitted::primary(FunctionTemplate { function: self }.render()?).with_aux(
            AuxChunk::Helper {
                id: self.helper_id.clone(),
                text: NativeFunctionTemplate { function: self }.render()?.into(),
            },
        );
        let emitted = match self.checks_status {
            true => emitted.with_aux(AuxChunk::ForwardDecl(StatusTemplate.render()?.into())),
            false => emitted,
        };
        let emitted = match self.requires_wire_runtime {
            true => emitted.with_aux(AuxChunk::ForwardDecl(WireTemplate.render()?.into())),
            false => emitted,
        };
        match &self.free_buffer_entry {
            Some(entry_point) => Ok(emitted.with_aux(AuxChunk::Helper {
                id: HelperId::new(CanonicalName::single("csharp_free_buffer")),
                text: FreeBufferTemplate { entry_point }.render()?.into(),
            })),
            None => Ok(emitted),
        }
    }
}

struct EncodedReturn {
    buffer: Identifier,
    reader: Identifier,
    decode: Expression,
}

struct EncodedError {
    buffer: Identifier,
    reader: Identifier,
    throw: Expression,
}

fn generated_identifier(source: &Identifier, suffix: &str) -> Result<Identifier> {
    Identifier::escape(format!(
        "{}{}",
        source.as_str().trim_start_matches('@'),
        suffix
    ))
}

fn lower_error(
    channel: ErrorChannel<'_, Native, OutOfRust>,
    type_namespace: Option<&Namespace>,
    c_function: &CFunction,
    context: &RenderContext<Native>,
) -> Result<Option<EncodedError>> {
    let ErrorChannel::Encoded {
        placement,
        ty,
        codec,
        shape,
    } = channel
    else {
        return match channel {
            ErrorChannel::None if c_function.return_channel() != ReturnChannel::EncodedError => {
                Ok(None)
            }
            ErrorChannel::None => broken_contract(
                "infallible function unexpectedly uses the C encoded-error return channel",
            ),
            ErrorChannel::Status => unsupported("status error channel"),
            _ => unsupported("unknown error channel"),
        };
    };
    if placement != ErrorPlacement::ReturnSlot
        || shape != native::BufferShape::Buffer
        || c_function.return_channel() != ReturnChannel::EncodedError
        || c_function.returns() != &CBridgeType::Buffer
    {
        return unsupported("encoded error channel shape");
    }

    let buffer = Identifier::parse("boltffiErrorBuffer")?;
    let reader = Identifier::parse("boltffiErrorReader")?;
    let mut codec_reader = Reader::new(reader.clone(), context);
    if let Some(namespace) = type_namespace {
        codec_reader = codec_reader.qualified(namespace);
    }
    let decode = codec
        .render_with(&mut codec_reader)
        .map(ReadExpression::into_expression)?;
    let throw = match ty {
        TypeRef::String => Expression::new(format!("new BoltException({decode})")),
        TypeRef::Record(_) | TypeRef::Enum(_) => {
            let ty = render_type_ref(ty, type_namespace, context)?;
            Expression::new(format!("new {ty}Exception({decode})"))
        }
        _ => return unsupported("encoded error type"),
    };
    Ok(Some(EncodedError {
        buffer,
        reader,
        throw,
    }))
}

fn render_callable_body(
    setup: &[Statement],
    invocation: &Expression,
    checks_status: bool,
    return_after_status: Option<&Expression>,
    encoded_return: Option<&EncodedReturn>,
    encoded_writeback: Option<&EncodedReturn>,
    encoded_error: Option<&EncodedError>,
) -> Result<Statement> {
    let mut lines = setup.iter().map(ToString::to_string).collect::<Vec<_>>();
    if let Some(error) = encoded_error {
        lines.push(format!(
            "FfiBuf {} = {invocation};\nif ({}.ptr != 0)\n{{\n    try\n    {{\n        WireReader {} = new WireReader({});\n        throw {};\n    }}\n    finally\n    {{\n        NativeMethods.FreeBuf({});\n    }}\n}}",
            error.buffer,
            error.buffer,
            error.reader,
            error.buffer,
            error.throw,
            error.buffer,
        ));
        if let Some(encoded) = encoded_return.or(encoded_writeback) {
            lines.push(render_buffer_return(encoded));
        } else if let Some(value) = return_after_status {
            lines.push(format!("return {value};"));
        }
        return Ok(Statement::new(indent(&lines.join("\n"), 12)));
    }

    match encoded_return {
        Some(encoded) => lines.push(format!(
            "FfiBuf {} = {invocation};\n{}",
            encoded.buffer,
            render_buffer_return(encoded),
        )),
        None if checks_status => {
            lines.push(format!(
                "FfiStatus status = {invocation};\nif (status.code != 0)\n{{\n    throw new global::System.InvalidOperationException($\"BoltFFI call failed with status code {{status.code}}\");\n}}"
            ));
            match (encoded_writeback, return_after_status) {
                (Some(encoded), _) => lines.push(render_buffer_return(encoded)),
                (None, Some(value)) => lines.push(format!("return {value};")),
                (None, None) => {}
            }
        }
        None => lines.push(format!("return {invocation};")),
    }
    Ok(Statement::new(indent(&lines.join("\n"), 12)))
}

fn render_buffer_return(encoded: &EncodedReturn) -> String {
    format!(
        "try\n{{\n    WireReader {} = new WireReader({});\n    return {};\n}}\nfinally\n{{\n    NativeMethods.FreeBuf({});\n}}",
        encoded.reader, encoded.buffer, encoded.decode, encoded.buffer,
    )
}

fn indent(source: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    source
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

struct LoweredReceiver {
    native_parameters: Vec<NativeParameter>,
    arguments: Vec<Expression>,
    return_after_status: Option<Expression>,
    encoded_writeback: Option<EncodedReturn>,
    setup: Vec<Statement>,
    requires_wire_runtime: bool,
}

#[allow(clippy::too_many_arguments)]
fn lower_receiver(
    owner: &DirectValueType,
    owner_name: &Identifier,
    receive: Receive,
    extension: bool,
    group: &ParameterGroup,
    c_function: &CFunction,
    bridge: &CBridgeContract,
) -> Result<LoweredReceiver> {
    let receiver_expression = Expression::new(match extension {
        true => "self",
        false => "this",
    });
    match (owner, receive, group) {
        (DirectValueType::Record(_), Receive::ByMutRef, ParameterGroup::DirectWriteback(group)) => {
            let input = c_function.parameter(group.input());
            let output = c_function.parameter(group.output());
            let output_matches = match output.ty() {
                CBridgeType::MutPointer(inner) => c_direct_matches(owner, inner, bridge)?,
                _ => false,
            };
            if !c_direct_matches(owner, input.ty(), bridge)? || !output_matches {
                return broken_contract("mutable record receiver does not match the C bridge");
            }
            let ty = TypeFragment::new(owner_name.to_string());
            let output_name = Identifier::parse("receiverOut")?;
            Ok(LoweredReceiver {
                native_parameters: vec![
                    NativeParameter {
                        name: Identifier::parse("receiver")?,
                        ty: ty.clone(),
                        modifier: "",
                        marshal_i1: false,
                        marshal_bool_array: false,
                        byte_array: false,
                    },
                    NativeParameter {
                        name: output_name.clone(),
                        ty: ty.clone(),
                        modifier: "out ",
                        marshal_i1: false,
                        marshal_bool_array: false,
                        byte_array: false,
                    },
                ],
                arguments: vec![
                    receiver_expression,
                    Expression::new(format!("out {ty} {output_name}")),
                ],
                return_after_status: Some(Expression::identifier(output_name)),
                encoded_writeback: None,
                setup: Vec::new(),
                requires_wire_runtime: false,
            })
        }
        (_, Receive::ByValue | Receive::ByRef, ParameterGroup::Value(index)) => {
            if !c_direct_matches(owner, c_function.parameter(*index).ty(), bridge)? {
                return broken_contract("value receiver does not match the C bridge");
            }
            Ok(LoweredReceiver {
                native_parameters: vec![NativeParameter {
                    name: Identifier::parse("receiver")?,
                    ty: TypeFragment::new(owner_name.to_string()),
                    modifier: "",
                    marshal_i1: false,
                    marshal_bool_array: false,
                    byte_array: false,
                }],
                arguments: vec![receiver_expression],
                return_after_status: None,
                encoded_writeback: None,
                setup: Vec::new(),
                requires_wire_runtime: false,
            })
        }
        (DirectValueType::Enum(_), Receive::ByMutRef, _) => unsupported("mutable enum receiver"),
        _ => broken_contract("method receiver does not match the C bridge"),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_encoded_receiver(
    receive: Receive,
    group: &ParameterGroup,
    plan: EncodedReceiverPlan<'_>,
    type_namespace: Option<&Namespace>,
    c_function: &CFunction,
    context: &RenderContext<Native>,
) -> Result<LoweredReceiver> {
    let (pointer, length, output) = match (receive, group) {
        (Receive::ByValue | Receive::ByRef, ParameterGroup::ByteSlice(group)) => {
            (group.pointer(), group.length(), None)
        }
        (Receive::ByMutRef, ParameterGroup::EncodedWriteback(group)) => {
            (group.pointer(), group.length(), Some(group.output()))
        }
        _ => return broken_contract("encoded receiver does not match the C bridge"),
    };
    if !matches!(
        c_function.parameter(pointer).ty(),
        CBridgeType::ConstPointer(inner) if inner.as_ref() == &CBridgeType::Uint8
    ) || c_function.parameter(length).ty() != &CBridgeType::PointerWidth
    {
        return broken_contract("encoded receiver byte slice does not match the C bridge");
    }
    if let Some(output) = output
        && c_function.parameter(output).ty()
            != &CBridgeType::MutPointer(Box::new(CBridgeType::Buffer))
    {
        return broken_contract("encoded receiver writeback does not match the C bridge");
    }

    let writer = Identifier::parse("boltffiReceiverWriter")?;
    let bytes = Identifier::parse("boltffiReceiverBytes")?;
    let mut setup = vec![Statement::new(format!(
        "WireWriter {writer} = new WireWriter();"
    ))];
    setup.extend(
        plan.write
            .render_with(&mut Writer::new(
                writer.clone(),
                Expression::new("this"),
                context,
            ))
            .into_iter()
            .collect::<Result<Vec<_>>>()?,
    );
    setup.push(Statement::new(format!(
        "byte[] {bytes} = {writer}.ToArray();"
    )));

    let mut native_parameters = vec![
        NativeParameter {
            name: bytes.clone(),
            ty: TypeFragment::new("byte[]"),
            modifier: "",
            marshal_i1: false,
            marshal_bool_array: false,
            byte_array: true,
        },
        NativeParameter {
            name: Identifier::parse("boltffiReceiverLength")?,
            ty: TypeFragment::new("nuint"),
            modifier: "",
            marshal_i1: false,
            marshal_bool_array: false,
            byte_array: false,
        },
    ];
    let mut arguments = vec![
        Expression::identifier(bytes.clone()),
        Expression::new(format!("(nuint){bytes}.Length")),
    ];

    let encoded_writeback = output
        .map(|_| -> Result<EncodedReturn> {
            let buffer = Identifier::parse("boltffiReceiverOut")?;
            native_parameters.push(NativeParameter {
                name: buffer.clone(),
                ty: TypeFragment::new("FfiBuf"),
                modifier: "out ",
                marshal_i1: false,
                marshal_bool_array: false,
                byte_array: false,
            });
            arguments.push(Expression::new(format!("out FfiBuf {buffer}")));
            let reader = Identifier::parse("boltffiReceiverReader")?;
            let mut codec_reader = Reader::new(reader.clone(), context);
            if let Some(namespace) = type_namespace {
                codec_reader = codec_reader.qualified(namespace);
            }
            let decode = plan
                .read
                .render_with(&mut codec_reader)
                .map(ReadExpression::into_expression)?;
            Ok(EncodedReturn {
                buffer,
                reader,
                decode,
            })
        })
        .transpose()?;

    Ok(LoweredReceiver {
        native_parameters,
        arguments,
        return_after_status: None,
        encoded_writeback,
        setup,
        requires_wire_runtime: true,
    })
}

pub(super) struct Module<'module> {
    namespace: &'module Namespace,
    class_name: Identifier,
    library: Literal,
}

impl<'module> Module<'module> {
    pub(super) fn new(
        namespace: &'module Namespace,
        class_name: Identifier,
        library: Literal,
    ) -> Self {
        Self {
            namespace,
            class_name,
            library,
        }
    }

    pub(super) fn render<'decl>(
        self,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Result<GeneratedOutput> {
        let mut functions = Vec::new();
        let mut native_functions = BTreeMap::<HelperId, Statement>::new();
        let mut support = BTreeMap::<String, Statement>::new();
        let mut diagnostics = Vec::<Diagnostic>::new();
        let mut files = Vec::<GeneratedFile>::new();

        for declaration in declarations {
            let declaration_ref = declaration.declaration();
            let (_, emitted) = declaration.into_parts();
            let (primary, aux, emitted_diagnostics) = emitted.into_parts();
            diagnostics.extend(emitted_diagnostics);
            let standalone = matches!(
                declaration_ref,
                DeclarationRef::Record(_) | DeclarationRef::Enum(_)
            );
            if standalone {
                let name = match declaration_ref {
                    DeclarationRef::Record(record) => record.name(),
                    DeclarationRef::Enum(enumeration) => enumeration.name(),
                    _ => unreachable!(),
                };
                files.push(GeneratedFile::new(
                    FilePath::new(format!("{}.cs", Name::new(name).pascal()?))?,
                    primary.into_string(),
                ));
            } else if !primary.is_empty() {
                functions.push(Statement::new(primary.into_string()));
            }
            for chunk in aux {
                match chunk {
                    AuxChunk::Helper { id, text } => {
                        native_functions
                            .entry(id)
                            .or_insert_with(|| Statement::new(text.into_string()));
                    }
                    AuxChunk::ForwardDecl(forward) => {
                        let forward = forward.into_string();
                        support
                            .entry(forward.clone())
                            .or_insert_with(|| Statement::new(forward));
                    }
                    AuxChunk::Import(_) => {
                        return Err(Error::UnexpectedBindingShape {
                            layer: "csharp module",
                            shape: "import auxiliary declaration",
                        });
                    }
                }
            }
        }

        let native_functions = native_functions.into_values().collect::<Vec<_>>();
        let support = support.into_values().collect::<Vec<_>>();
        let source = ModuleTemplate {
            namespace: self.namespace,
            class_name: &self.class_name,
            library: &self.library,
            support: &support,
            functions: &functions,
            native_functions: &native_functions,
        }
        .render()?;
        let path = FilePath::new(format!("{}.cs", self.class_name.as_str()))?;
        files.push(GeneratedFile::new(path, source));
        Ok(GeneratedOutput::new(files, diagnostics))
    }
}

fn bridge_function<'bridge>(
    symbol: &NativeSymbol,
    bridge: &'bridge CBridgeContract,
) -> Result<&'bridge CFunction> {
    let symbol = symbol.name().as_str();
    bridge
        .functions()
        .iter()
        .find(|function| function.name() == symbol)
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "function symbol is missing from the C bridge",
        })
}

fn direct_parameter_modifier(
    ty: &DirectValueType,
    receive: Receive,
    c_ty: &CBridgeType,
    bridge: &CBridgeContract,
) -> Result<&'static str> {
    match (ty, receive, c_ty) {
        (DirectValueType::Record(_), Receive::ByRef, CBridgeType::ConstPointer(inner))
            if c_direct_matches(ty, inner, bridge)? =>
        {
            Ok("in ")
        }
        (_, Receive::ByMutRef, _) => unsupported("mutable direct function parameters"),
        (_, Receive::ByValue | Receive::ByRef, _) if c_direct_matches(ty, c_ty, bridge)? => Ok(""),
        _ => broken_contract("function parameter type does not match the C bridge"),
    }
}

pub(in crate::target::csharp) fn primitive_type(primitive: Primitive) -> TypeFragment {
    TypeFragment::new(match primitive {
        Primitive::Bool => "bool",
        Primitive::I8 => "sbyte",
        Primitive::U8 => "byte",
        Primitive::I16 => "short",
        Primitive::U16 => "ushort",
        Primitive::I32 => "int",
        Primitive::U32 => "uint",
        Primitive::I64 => "long",
        Primitive::U64 => "ulong",
        Primitive::ISize => "nint",
        Primitive::USize => "nuint",
        Primitive::F32 => "float",
        Primitive::F64 => "double",
        _ => unreachable!("Primitive is exhaustively matched"),
    })
}

fn direct_type(ty: &DirectValueType, context: &RenderContext<Native>) -> Result<TypeFragment> {
    match ty {
        DirectValueType::Primitive(primitive) => Ok(primitive_type(*primitive)),
        DirectValueType::Record(id) => context
            .record(*id)
            .map(|record| Name::new(record.name()).pascal())
            .transpose()?
            .map(|name| TypeFragment::new(name.to_string()))
            .ok_or(Error::UnexpectedBindingShape {
                layer: "csharp function",
                shape: "missing direct record declaration",
            }),
        DirectValueType::Enum(id) => context
            .enumeration(*id)
            .map(|enumeration| Name::new(enumeration.name()).pascal())
            .transpose()?
            .map(|name| TypeFragment::new(name.to_string()))
            .ok_or(Error::UnexpectedBindingShape {
                layer: "csharp function",
                shape: "missing C-style enum declaration",
            }),
        _ => unsupported("unknown direct value type"),
    }
}

fn direct_type_with(
    ty: &DirectValueType,
    namespace: Option<&Namespace>,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    let rendered = direct_type(ty, context)?;
    Ok(match (ty, namespace) {
        (DirectValueType::Record(_) | DirectValueType::Enum(_), Some(namespace)) => {
            TypeFragment::new(format!("global::{namespace}.{rendered}"))
        }
        _ => rendered,
    })
}

fn direct_vector_element_type(
    element: &DirectVectorElementType,
    namespace: Option<&Namespace>,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    match element {
        DirectVectorElementType::Primitive(primitive) => Ok(primitive_type(primitive.primitive())),
        DirectVectorElementType::Record(record) => {
            direct_type_with(&DirectValueType::Record(*record), namespace, context)
        }
        _ => unsupported("direct-vector element type"),
    }
}

fn render_type_ref(
    ty: &boltffi_binding::TypeRef,
    namespace: Option<&Namespace>,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    match namespace {
        Some(namespace) => type_name::type_ref_qualified(ty, namespace, context),
        None => type_name::type_ref(ty, context),
    }
}

fn c_direct_matches(
    ty: &DirectValueType,
    c_ty: &CBridgeType,
    bridge: &CBridgeContract,
) -> Result<bool> {
    Ok(match (ty, c_ty) {
        (DirectValueType::Primitive(primitive), c_ty) => {
            c_ty == &CBridgeType::primitive(*primitive)?
        }
        (DirectValueType::Record(id), CBridgeType::DirectRecord(name)) => bridge
            .source_direct_record(*id)
            .is_some_and(|record| record.name() == name.as_str()),
        (DirectValueType::Enum(id), CBridgeType::CStyleEnum { name, .. }) => bridge
            .source_c_style_enum(*id)
            .is_some_and(|enumeration| enumeration.name() == name.as_str()),
        _ => false,
    })
}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: TARGET,
        shape,
    })
}

fn broken_contract<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}
