use askama::Template;
use boltffi_binding::{
    CallbackDecl, CanonicalName, DirectVectorElementType, ErrorChannel, ErrorPlacement,
    ExecutionDecl, HandlePresence, HandleTarget, ImportedMethodDecl, IntoRust, Native,
    OutgoingParam, ParamPlan, Primitive, ReturnPlan, TypeRef, VTableSlot, native,
};

use crate::{
    bridge::c::{CBridgeContract, CallbackSlot, ParameterGroup, Type as CBridgeType},
    core::{AuxChunk, Emitted, Error, HelperId, RenderContext, Result},
};

use super::super::{
    codec::{ReadExpression, Reader, Writer, primitive_read_method, primitive_write_method},
    name_style::{Name, Namespace},
    syntax::{Expression, Identifier, Statement, TypeFragment},
    type_name,
};
use super::{
    CallbackRuntimeTemplate, CopyBufferTemplate, FreeBufferTemplate, WireTemplate, direct_type,
    direct_vector_element_type, primitive_type,
};

#[derive(Template)]
#[template(path = "target/csharp/callback.cs", escape = "none")]
struct CallbackTemplate<'callback> {
    callback: &'callback Callback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::target::csharp) struct Callback {
    namespace: Namespace,
    name: Identifier,
    proxy_name: Identifier,
    bridge_name: Identifier,
    register_entry: String,
    create_entry: String,
    methods: Vec<CallbackMethod>,
    requires_wire_runtime: bool,
    free_buffer_entry: super::super::syntax::Literal,
    copy_buffer_entry: super::super::syntax::Literal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallbackMethod {
    name: Identifier,
    slot_name: Identifier,
    public_return_type: TypeFragment,
    returns_void: bool,
    parameters: Vec<CallbackParameter>,
    asynchronous: bool,
    native_return_type: TypeFragment,
    return_marshal_i1: bool,
    native_parameters: String,
    entry_body: Statement,
    proxy_body: Statement,
    completion_delegate: Option<String>,
    requires_wire_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallbackParameter {
    name: Identifier,
    ty: TypeFragment,
}

struct LoweredParameters {
    public: Vec<CallbackParameter>,
    entry_setup: Vec<String>,
    entry_arguments: Vec<String>,
    proxy_setup: Vec<String>,
    proxy_arguments: Vec<String>,
    proxy_cleanup: Vec<String>,
    wire: bool,
}

impl Callback {
    pub(in crate::target::csharp) fn from_declaration(
        declaration: &CallbackDecl<Native>,
        namespace: Namespace,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_callback =
            bridge
                .source_callback(declaration.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "callback protocol is missing from the C bridge",
                })?;
        let source_methods = declaration.protocol().vtable().methods();
        if source_methods.len() != c_callback.methods().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "callback method count does not match the C bridge",
            });
        }
        let name = Name::new(declaration.name()).pascal()?;
        let bridge_name = Identifier::parse(format!("{name}Bridge"))?;
        let methods = source_methods
            .iter()
            .zip(c_callback.methods())
            .map(|(method, slot)| {
                CallbackMethod::from_declaration(method, slot, &bridge_name, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            namespace,
            proxy_name: Identifier::parse(format!("{name}Proxy"))?,
            bridge_name,
            register_entry: declaration.protocol().register().name().as_str().to_owned(),
            create_entry: declaration
                .protocol()
                .create_handle()
                .name()
                .as_str()
                .to_owned(),
            requires_wire_runtime: methods.iter().any(CallbackMethod::requires_wire_runtime),
            free_buffer_entry: super::super::syntax::Literal::string(
                bridge.support().buffer_free()?.name(),
            ),
            copy_buffer_entry: super::super::syntax::Literal::string(
                bridge.support().buffer_from_bytes()?.name(),
            ),
            methods,
            name,
        })
    }

    pub(in crate::target::csharp) fn render(&self) -> Result<Emitted> {
        let mut emitted = Emitted::primary(CallbackTemplate { callback: self }.render()?).with_aux(
            AuxChunk::ForwardDecl(CallbackRuntimeTemplate.render()?.into()),
        );
        if self.requires_wire_runtime {
            emitted = emitted.with_aux(AuxChunk::ForwardDecl(WireTemplate.render()?.into()));
        }
        emitted = emitted
            .with_aux(AuxChunk::Helper {
                id: HelperId::new(CanonicalName::single("csharp_free_buffer")),
                text: FreeBufferTemplate {
                    entry_point: &self.free_buffer_entry,
                }
                .render()?
                .into(),
            })
            .with_aux(AuxChunk::Helper {
                id: HelperId::new(CanonicalName::single("csharp_copy_buffer")),
                text: CopyBufferTemplate {
                    entry_point: &self.copy_buffer_entry,
                }
                .render()?
                .into(),
            });
        Ok(emitted)
    }
}

impl CallbackMethod {
    fn from_declaration(
        declaration: &ImportedMethodDecl<Native, VTableSlot>,
        slot: &CallbackSlot,
        bridge_name: &Identifier,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Name::new(declaration.name()).pascal()?;
        let asynchronous = matches!(
            declaration.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        );
        let lowered = LoweredParameters::from_declaration(declaration, slot, bridge, context)?;
        let public_return_type =
            public_return_type(declaration.callable().returns().plan(), context)?;
        let native_return_type = c_type(slot.returns(), bridge, context)?;
        let return_marshal_i1 = slot.returns() == &CBridgeType::Bool;
        let completion_delegate = completion_delegate(&name, slot, bridge, context)?;
        let native_parameters = slot
            .parameters()
            .iter()
            .map(|parameter| {
                native_parameter(
                    parameter.name(),
                    parameter.ty(),
                    completion_delegate.as_ref().map(|_| &name),
                    bridge,
                    context,
                )
            })
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let error = declaration.callable().error().channel();
        let infallible = matches!(error, ErrorChannel::None);
        let fallible = matches!(error, ErrorChannel::Encoded { .. });
        let entry_body = match (asynchronous, infallible, fallible) {
            (false, true, _) => render_entry_body(declaration, &name, &lowered, slot, context)?,
            (false, _, true) => {
                render_fallible_entry_body(declaration, &name, &lowered, slot, context)?
            }
            (true, _, _) => {
                render_async_entry_body(declaration, &name, &lowered, slot, bridge, context)?
            }
            _ => unsupported_body(slot)?,
        };
        let proxy_body = match (asynchronous, infallible, fallible) {
            (false, true, _) => {
                render_proxy_body(declaration, &name, bridge_name, &lowered, context)?
            }
            (false, _, true) => render_fallible_proxy_body(
                declaration,
                &name,
                bridge_name,
                &lowered,
                slot,
                context,
            )?,
            (true, _, _) => {
                render_async_proxy_body(declaration, &name, bridge_name, &lowered, slot, context)?
            }
            _ => Statement::new(
                "            throw new global::System.NotSupportedException(\"This callback method shape has not migrated\");",
            ),
        };
        Ok(Self {
            name,
            slot_name: Identifier::escape(slot.name().as_str())?,
            public_return_type,
            returns_void: matches!(declaration.callable().returns().plan(), ReturnPlan::Void),
            parameters: lowered.public,
            asynchronous,
            native_return_type,
            return_marshal_i1,
            native_parameters,
            entry_body,
            proxy_body,
            completion_delegate,
            requires_wire_runtime: lowered.wire,
        })
    }

    fn requires_wire_runtime(&self) -> bool {
        self.requires_wire_runtime
            || self.native_return_type.to_string() == "FfiBuf"
            || self.native_parameters.contains("FfiBuf")
    }
}

impl LoweredParameters {
    fn from_declaration(
        declaration: &ImportedMethodDecl<Native, VTableSlot>,
        slot: &CallbackSlot,
        _bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let mut lowered = Self {
            public: Vec::new(),
            entry_setup: Vec::new(),
            entry_arguments: Vec::new(),
            proxy_setup: Vec::new(),
            proxy_arguments: Vec::new(),
            proxy_cleanup: Vec::new(),
            wire: false,
        };
        if declaration.callable().params().len() != slot.source_parameter_groups().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "callback source parameter count does not match the C bridge",
            });
        }
        for (parameter, group) in declaration
            .callable()
            .params()
            .iter()
            .zip(slot.source_parameter_groups())
        {
            let OutgoingParam::Value(plan) = parameter.payload() else {
                return super::unsupported("callback closure parameter");
            };
            let name = Name::new(parameter.name()).camel()?;
            match plan {
                ParamPlan::Direct { ty, .. } => {
                    let ParameterGroup::Value(index) = group else {
                        return broken_callback("direct callback parameter group");
                    };
                    let native_name = Identifier::escape(slot.parameter(*index).name())?;
                    lowered.public.push(CallbackParameter {
                        name: name.clone(),
                        ty: direct_type(ty, context)?,
                    });
                    lowered.entry_arguments.push(native_name.to_string());
                    lowered.proxy_arguments.push(name.to_string());
                }
                ParamPlan::Encoded { ty, codec, .. } => {
                    let ParameterGroup::ByteSlice(slice) = group else {
                        return broken_callback("encoded callback parameter group");
                    };
                    lowered.wire = true;
                    let pointer = Identifier::escape(slot.parameter(slice.pointer()).name())?;
                    let length = Identifier::escape(slot.parameter(slice.length()).name())?;
                    let reader = Identifier::parse(format!("boltffi{name}Reader"))?;
                    let decode = codec
                        .render_with(&mut Reader::new(reader.clone(), context))
                        .map(ReadExpression::into_expression)?;
                    lowered.public.push(CallbackParameter {
                        name: name.clone(),
                        ty: type_name::type_ref(ty, context)?,
                    });
                    lowered.entry_setup.push(format!(
                        "WireReader {reader} = new WireReader({pointer}, {length});"
                    ));
                    lowered.entry_arguments.push(decode.to_string());
                    let writer = Identifier::parse(format!("boltffi{name}Writer"))?;
                    let bytes = Identifier::parse(format!("boltffi{name}Bytes"))?;
                    let pin = Identifier::parse(format!("boltffi{name}Pin"))?;
                    let ptr = Identifier::parse(format!("boltffi{name}Ptr"))?;
                    let writes = codec
                        .write_self_value()
                        .render_with(&mut Writer::new(
                            writer.clone(),
                            Expression::identifier(name.clone()),
                            context,
                        ))
                        .into_iter()
                        .collect::<Result<Vec<_>>>()?;
                    lowered.proxy_setup.push(format!(
                        "WireWriter {writer} = new WireWriter();\n{}\nbyte[] {bytes} = {writer}.ToArray();\nglobal::System.Runtime.InteropServices.GCHandle {pin} = default;\nnint {ptr} = 0;",
                        writes
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                    lowered.proxy_setup.push(format!(
                        "if ({bytes}.Length != 0)\n{{\n    {pin} = global::System.Runtime.InteropServices.GCHandle.Alloc({bytes}, global::System.Runtime.InteropServices.GCHandleType.Pinned);\n    {ptr} = {pin}.AddrOfPinnedObject();\n}}"
                    ));
                    lowered
                        .proxy_arguments
                        .extend([ptr.to_string(), format!("(nuint){bytes}.Length")]);
                    lowered
                        .proxy_cleanup
                        .push(format!("if ({pin}.IsAllocated) {pin}.Free();"));
                }
                ParamPlan::ScalarOption { primitive } => {
                    let ParameterGroup::ByteSlice(slice) = group else {
                        return broken_callback("scalar-option callback parameter group");
                    };
                    lowered.wire = true;
                    let pointer = Identifier::escape(slot.parameter(slice.pointer()).name())?;
                    let length = Identifier::escape(slot.parameter(slice.length()).name())?;
                    let reader = Identifier::parse(format!("boltffi{name}Reader"))?;
                    lowered.public.push(CallbackParameter {
                        name: name.clone(),
                        ty: TypeFragment::new(format!("{}?", primitive_type(*primitive))),
                    });
                    lowered.entry_setup.push(format!(
                        "WireReader {reader} = new WireReader({pointer}, {length});"
                    ));
                    lowered.entry_arguments.push(format!(
                        "{reader}.ReadU8() == 0 ? default({}?) : {reader}.{}()",
                        primitive_type(*primitive),
                        primitive_read_method(*primitive)
                    ));
                    let writer = Identifier::parse(format!("boltffi{name}Writer"))?;
                    let bytes = Identifier::parse(format!("boltffi{name}Bytes"))?;
                    let pin = Identifier::parse(format!("boltffi{name}Pin"))?;
                    let ptr = Identifier::parse(format!("boltffi{name}Ptr"))?;
                    lowered.proxy_setup.push(format!(
                        "WireWriter {writer} = new WireWriter();\nif ({name}.HasValue)\n{{\n    {writer}.WriteU8(1);\n    {writer}.{}({name}.Value);\n}}\nelse\n{{\n    {writer}.WriteU8(0);\n}}\nbyte[] {bytes} = {writer}.ToArray();\nglobal::System.Runtime.InteropServices.GCHandle {pin} = global::System.Runtime.InteropServices.GCHandle.Alloc({bytes}, global::System.Runtime.InteropServices.GCHandleType.Pinned);\nnint {ptr} = {pin}.AddrOfPinnedObject();",
                        primitive_write_method(*primitive)
                    ));
                    lowered
                        .proxy_arguments
                        .extend([ptr.to_string(), format!("(nuint){bytes}.Length")]);
                    lowered.proxy_cleanup.push(format!("{pin}.Free();"));
                }
                ParamPlan::DirectVec { element, .. } => {
                    let ParameterGroup::DirectVector(vector) = group else {
                        return broken_callback("direct-vector callback parameter group");
                    };
                    lowered.wire = true;
                    let pointer = Identifier::escape(slot.parameter(vector.pointer()).name())?;
                    let length = Identifier::escape(slot.parameter(vector.length()).name())?;
                    let element_type = direct_vector_element_type(element, None, context)?;
                    let reader = Identifier::parse(format!("boltffi{name}Reader"))?;
                    lowered.public.push(CallbackParameter {
                        name: name.clone(),
                        ty: TypeFragment::new(format!("{element_type}[]")),
                    });
                    let byte_length = match element {
                        DirectVectorElementType::Primitive(primitive)
                            if primitive.primitive() == Primitive::Bool =>
                        {
                            length.to_string()
                        }
                        DirectVectorElementType::Primitive(_) => format!(
                            "checked({length} * (nuint)global::System.Runtime.CompilerServices.Unsafe.SizeOf<{element_type}>())"
                        ),
                        DirectVectorElementType::Record(_) => length.to_string(),
                        _ => return super::unsupported("callback direct-vector element type"),
                    };
                    lowered.entry_setup.push(format!(
                        "WireReader {reader} = new WireReader({pointer}, {byte_length});"
                    ));
                    lowered.entry_arguments.push(match element {
                        DirectVectorElementType::Primitive(primitive)
                            if primitive.primitive() == Primitive::Bool =>
                        {
                            format!("{reader}.ReadRawBoolArray()")
                        }
                        _ => format!("{reader}.ReadRawArray<{element_type}>()"),
                    });
                    let pin = Identifier::parse(format!("boltffi{name}Pin"))?;
                    let ptr = Identifier::parse(format!("boltffi{name}Ptr"))?;
                    let (pinned_value, length) = match element {
                        DirectVectorElementType::Primitive(primitive)
                            if primitive.primitive() == Primitive::Bool =>
                        {
                            let bytes = Identifier::parse(format!("boltffi{name}Bytes"))?;
                            lowered.proxy_setup.push(format!(
                                "byte[] {bytes} = new byte[{name}.Length];\nfor (int boltffiIndex = 0; boltffiIndex < {name}.Length; boltffiIndex++) {bytes}[boltffiIndex] = {name}[boltffiIndex] ? (byte)1 : (byte)0;"
                            ));
                            (bytes.to_string(), format!("(nuint){name}.Length"))
                        }
                        DirectVectorElementType::Primitive(_) => {
                            (name.to_string(), format!("(nuint){name}.Length"))
                        }
                        DirectVectorElementType::Record(_) => (
                            name.to_string(),
                            format!(
                                "checked((nuint){name}.Length * (nuint)global::System.Runtime.CompilerServices.Unsafe.SizeOf<{element_type}>())"
                            ),
                        ),
                        _ => return super::unsupported("callback direct-vector element type"),
                    };
                    lowered.proxy_setup.push(format!(
                        "global::System.Runtime.InteropServices.GCHandle {pin} = default;\nnint {ptr} = 0;\nif ({name}.Length != 0)\n{{\n    {pin} = global::System.Runtime.InteropServices.GCHandle.Alloc({pinned_value}, global::System.Runtime.InteropServices.GCHandleType.Pinned);\n    {ptr} = {pin}.AddrOfPinnedObject();\n}}"
                    ));
                    lowered.proxy_arguments.extend([ptr.to_string(), length]);
                    lowered
                        .proxy_cleanup
                        .push(format!("if ({pin}.IsAllocated) {pin}.Free();"));
                }
                ParamPlan::Handle {
                    target, presence, ..
                } => {
                    let ParameterGroup::Value(index) = group else {
                        return broken_callback("handle callback parameter group");
                    };
                    let native_name = Identifier::escape(slot.parameter(*index).name())?;
                    let (ty, entry, proxy) = match target {
                        HandleTarget::Class(class) => {
                            let ty = type_name::class(*class, context)?;
                            (
                                ty.clone(),
                                match presence {
                                    HandlePresence::Required => format!("new {ty}({native_name})"),
                                    HandlePresence::Nullable => format!(
                                        "{native_name} == 0 ? null : new {ty}({native_name})"
                                    ),
                                    _ => return super::unsupported("callback handle presence"),
                                },
                                match presence {
                                    HandlePresence::Required => format!("{name}.Handle"),
                                    HandlePresence::Nullable => format!("{name}?.Handle ?? 0"),
                                    _ => return super::unsupported("callback handle presence"),
                                },
                            )
                        }
                        HandleTarget::Callback(callback) => {
                            let ty = type_name::callback(*callback, context)?;
                            (
                                ty.clone(),
                                match presence {
                                    HandlePresence::Required => {
                                        format!("{ty}Bridge.Wrap({native_name})")
                                    }
                                    HandlePresence::Nullable => format!(
                                        "{native_name}.IsNull ? null : {ty}Bridge.Wrap({native_name})"
                                    ),
                                    _ => return super::unsupported("callback handle presence"),
                                },
                                format!("{ty}Bridge.Create({name})"),
                            )
                        }
                        _ => return super::unsupported("stream callback handle parameter"),
                    };
                    lowered.public.push(CallbackParameter {
                        name: name.clone(),
                        ty: match presence {
                            HandlePresence::Required => ty,
                            HandlePresence::Nullable => TypeFragment::new(format!("{ty}?")),
                            _ => return super::unsupported("callback handle presence"),
                        },
                    });
                    lowered.entry_arguments.push(entry);
                    lowered.proxy_arguments.push(proxy);
                }
                _ => return super::unsupported("unknown callback parameter"),
            }
        }
        Ok(lowered)
    }
}

fn render_entry_body(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    method_name: &Identifier,
    parameters: &LoweredParameters,
    slot: &CallbackSlot,
    context: &RenderContext<Native>,
) -> Result<Statement> {
    let arguments = parameters.entry_arguments.join(", ");
    let mut body = vec![format!(
        "if (!Handles.TryGetValue(handle, out var implementation)) {}",
        match slot.returns() {
            CBridgeType::Void => "return;".to_owned(),
            _ => "return default;".to_owned(),
        }
    )];
    body.extend(parameters.entry_setup.iter().cloned());
    let call = format!("implementation.{method_name}({arguments})");
    match declaration.callable().returns().plan() {
        ReturnPlan::Void => body.push(format!("{call};")),
        ReturnPlan::DirectViaReturnSlot { .. } => body.push(format!("return {call};")),
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            let writer = Identifier::parse("boltffiReturnWriter")?;
            let value = Identifier::parse("boltffiValue")?;
            let writes = codec
                .render_with(&mut Writer::new(
                    writer.clone(),
                    Expression::identifier(value.clone()),
                    context,
                ))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            body.push(format!("var {value} = {call};"));
            body.push(format!("WireWriter {writer} = new WireWriter();"));
            body.extend(writes.into_iter().map(|statement| statement.to_string()));
            body.push(format!("return FfiBuf.FromBytes({writer}.ToArray());"));
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            body.push(format!("var boltffiValue = {call};"));
            body.push("WireWriter boltffiReturnWriter = new WireWriter();".to_owned());
            body.push(format!(
                "if (boltffiValue.HasValue)\n{{\n    boltffiReturnWriter.WriteU8(1);\n    boltffiReturnWriter.{}(boltffiValue.Value);\n}}\nelse\n{{\n    boltffiReturnWriter.WriteU8(0);\n}}",
                primitive_write_method(*primitive)
            ));
            body.push("return FfiBuf.FromBytes(boltffiReturnWriter.ToArray());".to_owned());
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            body.push(format!("var boltffiValue = {call};"));
            body.push(match element {
                DirectVectorElementType::Primitive(primitive)
                    if primitive.primitive() == Primitive::Bool =>
                {
                    "return FfiBuf.FromRawBoolArray(boltffiValue);".to_owned()
                }
                _ => "return FfiBuf.FromRawArray(boltffiValue);".to_owned(),
            });
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        } => {
            let value = format!("var boltffiValue = {call};");
            body.push(value);
            body.push(match target {
                HandleTarget::Class(_) => match presence {
                    HandlePresence::Required => "return boltffiValue.Handle;".to_owned(),
                    HandlePresence::Nullable => "return boltffiValue?.Handle ?? 0;".to_owned(),
                    _ => return super::unsupported("callback handle return presence"),
                },
                HandleTarget::Callback(callback) => {
                    let ty = type_name::callback(*callback, context)?;
                    format!("return {ty}Bridge.Create(boltffiValue);")
                }
                _ => return super::unsupported("stream callback handle return"),
            });
        }
        _ => return super::unsupported("callback return shape"),
    }
    Ok(Statement::new(indent_lines(&body, 12)))
}

fn render_fallible_entry_body(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    method_name: &Identifier,
    parameters: &LoweredParameters,
    slot: &CallbackSlot,
    context: &RenderContext<Native>,
) -> Result<Statement> {
    let ErrorChannel::Encoded {
        placement: ErrorPlacement::ReturnSlot,
        ty: error_type,
        codec: error_codec,
        shape: native::BufferShape::Buffer,
    } = declaration.callable().error().channel()
    else {
        return super::unsupported("callback encoded error channel");
    };
    if slot.returns() != &CBridgeType::Buffer {
        return broken_callback("fallible callback error return slot");
    }
    let success_out = match slot.return_parameter_groups() {
        [] if matches!(declaration.callable().returns().plan(), ReturnPlan::Void) => None,
        [ParameterGroup::SuccessOut(index)] => {
            Some(Identifier::escape(slot.parameter(*index).name())?)
        }
        _ => return broken_callback("fallible callback success out parameter"),
    };
    let arguments = parameters.entry_arguments.join(", ");
    let call = format!("implementation.{method_name}({arguments})");
    let mut success = vec![
        "if (!Handles.TryGetValue(handle, out var implementation)) throw new global::System.InvalidOperationException(\"invalid callback handle\");".to_owned(),
    ];
    success.extend(parameters.entry_setup.iter().cloned());
    match declaration.callable().returns().plan() {
        ReturnPlan::Void => success.push(format!("{call};")),
        ReturnPlan::DirectViaOutPointer { .. } => success.push(format!(
            "{} = {call};",
            success_out.as_ref().ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "fallible direct callback success out is missing",
            })?
        )),
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let out = success_out.as_ref().ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "fallible encoded callback success out is missing",
            })?;
            let writer = Identifier::parse("boltffiSuccessWriter")?;
            let value = Identifier::parse("boltffiValue")?;
            let writes = codec
                .render_with(&mut Writer::new(
                    writer.clone(),
                    Expression::identifier(value.clone()),
                    context,
                ))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            success.push(format!("var {value} = {call};"));
            success.push(format!("WireWriter {writer} = new WireWriter();"));
            success.extend(writes.into_iter().map(|statement| statement.to_string()));
            success.push(format!("{out} = FfiBuf.FromBytes({writer}.ToArray());"));
        }
        _ => return super::unsupported("fallible callback success return shape"),
    }
    success.push("return default;".to_owned());

    let error = Identifier::parse("boltffiError")?;
    let writer = Identifier::parse("boltffiErrorWriter")?;
    let (exception_type, error_value) = callback_error_exception(error_type, &error, context)?;
    let writes = error_codec
        .render_with(&mut Writer::new(writer.clone(), error_value, context))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let mut failure = vec![format!("WireWriter {writer} = new WireWriter();")];
    failure.extend(writes.into_iter().map(|statement| statement.to_string()));
    failure.push(format!("return FfiBuf.FromBytes({writer}.ToArray());"));

    let mut body = Vec::new();
    if let Some(out) = success_out {
        body.push(format!("{out} = default;"));
    }
    body.push(format!(
        "try\n{{\n{}\n}}\ncatch ({exception_type} {error})\n{{\n{}\n}}",
        indent_lines(&success, 4),
        indent_lines(&failure, 4),
    ));
    Ok(Statement::new(indent_lines(&body, 12)))
}

fn render_proxy_body(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    method_name: &Identifier,
    bridge_name: &Identifier,
    parameters: &LoweredParameters,
    context: &RenderContext<Native>,
) -> Result<Statement> {
    let slot_name = Name::new(declaration.name()).snake();
    let mut before = vec![
        "if (handle.IsNull) throw new global::System.ObjectDisposedException(GetType().Name);"
            .to_owned(),
        format!(
            "{bridge_name}.{method_name}Fn invoke = global::System.Runtime.InteropServices.Marshal.GetDelegateForFunctionPointer<{bridge_name}.{method_name}Fn>(vtable.{slot_name});"
        ),
    ];
    before.extend(parameters.proxy_setup.iter().cloned());
    let mut arguments = vec!["handle.handle".to_owned()];
    arguments.extend(parameters.proxy_arguments.iter().cloned());
    let call = format!("invoke({})", arguments.join(", "));
    let call_body = match declaration.callable().returns().plan() {
        ReturnPlan::Void => format!("{call};"),
        ReturnPlan::DirectViaReturnSlot { .. } => format!("return {call};"),
        ReturnPlan::EncodedViaReturnSlot { codec, .. } => {
            let reader = Identifier::parse("boltffiReturnReader")?;
            let decode = codec
                .read_plan()
                .render_with(&mut Reader::new(reader.clone(), context))
                .map(ReadExpression::into_expression)?;
            format!(
                "FfiBuf boltffiBuffer = {call};\ntry\n{{\n    WireReader {reader} = new WireReader(boltffiBuffer);\n    return {decode};\n}}\nfinally\n{{\n    NativeMethods.FreeBuf(boltffiBuffer);\n}}"
            )
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => format!(
            "FfiBuf boltffiBuffer = {call};\ntry\n{{\n    WireReader boltffiReturnReader = new WireReader(boltffiBuffer);\n    return boltffiReturnReader.ReadU8() == 0 ? default({}?) : boltffiReturnReader.{}();\n}}\nfinally\n{{\n    NativeMethods.FreeBuf(boltffiBuffer);\n}}",
            primitive_type(*primitive),
            primitive_read_method(*primitive)
        ),
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let element_type = direct_vector_element_type(element, None, context)?;
            let decode = match element {
                DirectVectorElementType::Primitive(primitive)
                    if primitive.primitive() == Primitive::Bool =>
                {
                    "boltffiReturnReader.ReadRawBoolArray()".to_owned()
                }
                _ => format!("boltffiReturnReader.ReadRawArray<{element_type}>()"),
            };
            format!(
                "FfiBuf boltffiBuffer = {call};\ntry\n{{\n    WireReader boltffiReturnReader = new WireReader(boltffiBuffer);\n    return {decode};\n}}\nfinally\n{{\n    NativeMethods.FreeBuf(boltffiBuffer);\n}}"
            )
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        } => {
            let ty = match target {
                HandleTarget::Class(class) => type_name::class(*class, context)?,
                HandleTarget::Callback(callback) => type_name::callback(*callback, context)?,
                _ => return super::unsupported("stream callback handle return"),
            };
            let native = format!("var boltffiHandle = {call};");
            let wrap = match target {
                HandleTarget::Class(_) => match presence {
                    HandlePresence::Required => format!("return new {ty}(boltffiHandle);"),
                    HandlePresence::Nullable => {
                        format!("return boltffiHandle == 0 ? null : new {ty}(boltffiHandle);")
                    }
                    _ => return super::unsupported("callback handle return presence"),
                },
                HandleTarget::Callback(_) => match presence {
                    HandlePresence::Required => {
                        format!("return {ty}Bridge.Wrap(boltffiHandle);")
                    }
                    HandlePresence::Nullable => format!(
                        "return boltffiHandle.IsNull ? null : {ty}Bridge.Wrap(boltffiHandle);"
                    ),
                    _ => return super::unsupported("callback handle return presence"),
                },
                _ => unreachable!(),
            };
            format!("{native}\n{wrap}")
        }
        _ => return super::unsupported("callback proxy return shape"),
    };
    if parameters.proxy_cleanup.is_empty() {
        before.push(call_body);
    } else {
        before.push(format!(
            "try\n{{\n{}\n}}\nfinally\n{{\n{}\n}}",
            indent_lines(&[call_body], 4),
            indent_lines(&parameters.proxy_cleanup, 4),
        ));
    }
    Ok(Statement::new(indent_lines(&before, 12)))
}

fn render_fallible_proxy_body(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    method_name: &Identifier,
    bridge_name: &Identifier,
    parameters: &LoweredParameters,
    slot: &CallbackSlot,
    context: &RenderContext<Native>,
) -> Result<Statement> {
    let ErrorChannel::Encoded {
        placement: ErrorPlacement::ReturnSlot,
        ty: error_type,
        codec: error_codec,
        shape: native::BufferShape::Buffer,
    } = declaration.callable().error().channel()
    else {
        return super::unsupported("callback encoded error channel");
    };
    let success_out = match slot.return_parameter_groups() {
        [] if matches!(declaration.callable().returns().plan(), ReturnPlan::Void) => None,
        [ParameterGroup::SuccessOut(index)] => {
            Some(Identifier::escape(slot.parameter(*index).name())?)
        }
        _ => return broken_callback("fallible callback success out parameter"),
    };
    let slot_name = Name::new(declaration.name()).snake();
    let mut before = vec![
        "if (handle.IsNull) throw new global::System.ObjectDisposedException(GetType().Name);"
            .to_owned(),
        format!(
            "{bridge_name}.{method_name}Fn invoke = global::System.Runtime.InteropServices.Marshal.GetDelegateForFunctionPointer<{bridge_name}.{method_name}Fn>(vtable.{slot_name});"
        ),
    ];
    before.extend(parameters.proxy_setup.iter().cloned());
    let mut arguments = vec![None; slot.parameters().len()];
    arguments[0] = Some("handle.handle".to_owned());
    let mut source_arguments = parameters.proxy_arguments.iter();
    for group in slot.source_parameter_groups() {
        for index in callback_group_indices(group)? {
            arguments[index] = Some(
                source_arguments
                    .next()
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "callback proxy source argument is missing",
                    })?
                    .clone(),
            );
        }
    }
    if source_arguments.next().is_some() {
        return broken_callback("callback proxy source argument count");
    }
    if let Some(out) = &success_out {
        let [ParameterGroup::SuccessOut(index)] = slot.return_parameter_groups() else {
            return broken_callback("fallible callback success out parameter");
        };
        arguments[index.position()] = Some(format!("out var {out}"));
    }
    let arguments =
        arguments
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "callback proxy native argument is missing",
            })?;
    let call = format!("invoke({})", arguments.join(", "));
    let error_reader = Identifier::parse("boltffiErrorReader")?;
    let decode_error = error_codec
        .read_plan()
        .render_with(&mut Reader::new(error_reader.clone(), context))
        .map(ReadExpression::into_expression)?;
    let throw = callback_error_throw(error_type, decode_error, context)?;
    let mut call_body = vec![format!("FfiBuf boltffiErrorBuffer = {call};")];
    call_body.push(format!(
        "if (boltffiErrorBuffer.ptr != 0)\n{{\n    try\n    {{\n        WireReader {error_reader} = new WireReader(boltffiErrorBuffer);\n        throw {throw};\n    }}\n    finally\n    {{\n        NativeMethods.FreeBuf(boltffiErrorBuffer);\n    }}\n}}"
    ));
    match declaration.callable().returns().plan() {
        ReturnPlan::Void => {}
        ReturnPlan::DirectViaOutPointer { .. } => call_body.push(format!(
            "return {};",
            success_out.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "fallible direct callback success out is missing",
            })?
        )),
        ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let out = success_out.ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "fallible encoded callback success out is missing",
            })?;
            let reader = Identifier::parse("boltffiSuccessReader")?;
            let decode = codec
                .read_plan()
                .render_with(&mut Reader::new(reader.clone(), context))
                .map(ReadExpression::into_expression)?;
            call_body.push(format!(
                "try\n{{\n    WireReader {reader} = new WireReader({out});\n    return {decode};\n}}\nfinally\n{{\n    NativeMethods.FreeBuf({out});\n}}"
            ));
        }
        _ => return super::unsupported("fallible callback proxy success shape"),
    }
    let call_body = call_body.join("\n");
    if parameters.proxy_cleanup.is_empty() {
        before.push(call_body);
    } else {
        before.push(format!(
            "try\n{{\n{}\n}}\nfinally\n{{\n{}\n}}",
            indent_lines(&[call_body], 4),
            indent_lines(&parameters.proxy_cleanup, 4),
        ));
    }
    Ok(Statement::new(indent_lines(&before, 12)))
}

fn callback_error_exception(
    ty: &TypeRef,
    error: &Identifier,
    context: &RenderContext<Native>,
) -> Result<(TypeFragment, Expression)> {
    match ty {
        TypeRef::String => Ok((
            TypeFragment::new("global::System.Exception"),
            Expression::new(format!("{error}.Message")),
        )),
        TypeRef::Record(_) | TypeRef::Enum(_) => {
            let ty = type_name::type_ref(ty, context)?;
            Ok((
                TypeFragment::new(format!("{ty}Exception")),
                Expression::new(format!("{error}.Error")),
            ))
        }
        _ => super::unsupported("callback encoded error type"),
    }
}

fn callback_group_indices(group: &ParameterGroup) -> Result<Vec<usize>> {
    match group {
        ParameterGroup::Value(index) => Ok(vec![index.position()]),
        ParameterGroup::ByteSlice(slice) => {
            Ok(vec![slice.pointer().position(), slice.length().position()])
        }
        ParameterGroup::DirectVector(vector) => Ok(vec![
            vector.pointer().position(),
            vector.length().position(),
        ]),
        _ => broken_callback("callback source parameter group"),
    }
}

fn callback_error_throw(
    ty: &TypeRef,
    decoded: Expression,
    context: &RenderContext<Native>,
) -> Result<Expression> {
    match ty {
        TypeRef::String => Ok(Expression::new(format!("new BoltException({decoded})"))),
        TypeRef::Record(_) | TypeRef::Enum(_) => {
            let ty = type_name::type_ref(ty, context)?;
            Ok(Expression::new(format!("new {ty}Exception({decoded})")))
        }
        _ => super::unsupported("callback encoded error type"),
    }
}

fn public_return_type(
    plan: &ReturnPlan<Native, IntoRust>,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    match plan {
        ReturnPlan::Void => Ok(TypeFragment::void()),
        ReturnPlan::DirectViaReturnSlot { ty } | ReturnPlan::DirectViaOutPointer { ty } => {
            direct_type(ty, context)
        }
        ReturnPlan::EncodedViaReturnSlot { ty, .. }
        | ReturnPlan::EncodedViaOutPointer { ty, .. } => type_name::type_ref(ty, context),
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => {
            let ty = match target {
                HandleTarget::Class(class) => type_name::class(*class, context)?,
                HandleTarget::Callback(callback) => type_name::callback(*callback, context)?,
                _ => return super::unsupported("callback stream handle return"),
            };
            Ok(match presence {
                HandlePresence::Required => ty,
                HandlePresence::Nullable => TypeFragment::new(format!("{ty}?")),
                _ => return super::unsupported("callback handle presence"),
            })
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => Ok(TypeFragment::new(format!(
            "{}?",
            primitive_type(*primitive)
        ))),
        ReturnPlan::DirectVecViaReturnSlot { element } => Ok(TypeFragment::new(format!(
            "{}[]",
            direct_vector_element_type(element, None, context)?
        ))),
        _ => super::unsupported("callback public return type"),
    }
}

fn c_type(
    ty: &CBridgeType,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<TypeFragment> {
    Ok(match ty {
        CBridgeType::Void => TypeFragment::void(),
        CBridgeType::Bool => TypeFragment::new("bool"),
        CBridgeType::Int8 => TypeFragment::new("sbyte"),
        CBridgeType::Uint8 => TypeFragment::new("byte"),
        CBridgeType::Int16 => TypeFragment::new("short"),
        CBridgeType::Uint16 => TypeFragment::new("ushort"),
        CBridgeType::Int32 => TypeFragment::new("int"),
        CBridgeType::Uint32 => TypeFragment::new("uint"),
        CBridgeType::Int64 => TypeFragment::new("long"),
        CBridgeType::Uint64 => TypeFragment::new("ulong"),
        CBridgeType::Float32 => TypeFragment::new("float"),
        CBridgeType::Float64 => TypeFragment::new("double"),
        CBridgeType::SignedPointerWidth => TypeFragment::new("nint"),
        CBridgeType::PointerWidth => TypeFragment::new("nuint"),
        CBridgeType::Status => TypeFragment::new("FfiStatus"),
        CBridgeType::Buffer => TypeFragment::new("FfiBuf"),
        CBridgeType::FutureHandle | CBridgeType::ConstPointer(_) | CBridgeType::MutPointer(_) => {
            TypeFragment::new("nint")
        }
        CBridgeType::CallbackHandle(_) => TypeFragment::new("BoltFFICallbackHandle"),
        CBridgeType::DirectRecord(name) => {
            let (id, _) = bridge
                .source_direct_records()
                .iter()
                .find(|(_, record)| record.name() == name.as_str())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "callback direct record type is missing",
                })?;
            type_name::record(*id, context)?
        }
        CBridgeType::CStyleEnum { name, .. } => {
            let (id, _) = bridge
                .source_c_style_enums()
                .iter()
                .find(|(_, enumeration)| enumeration.name() == name.as_str())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: "c",
                    invariant: "callback C-style enum type is missing",
                })?;
            type_name::enumeration(*id, context)?
        }
        _ => return super::unsupported("callback C ABI type"),
    })
}

fn native_parameter(
    name: &str,
    ty: &CBridgeType,
    completion: Option<&Identifier>,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<String> {
    let name = Identifier::escape(name)?;
    match ty {
        CBridgeType::MutPointer(inner) if inner.as_ref() == &CBridgeType::Void => {
            Ok(format!("nint {name}"))
        }
        CBridgeType::MutPointer(inner) => Ok(format!(
            "{}out {} {name}",
            bool_parameter_attribute(inner),
            c_type(inner, bridge, context)?
        )),
        CBridgeType::FunctionPointer { .. } => completion
            .map(|method| format!("{method}Completion {name}"))
            .ok_or(Error::UnsupportedTarget {
                target: "csharp",
                shape: "callback function pointer parameter",
            }),
        CBridgeType::Bool => Ok(format!("{}bool {name}", bool_parameter_attribute(ty))),
        _ => Ok(format!("{} {name}", c_type(ty, bridge, context)?)),
    }
}

fn completion_delegate(
    method: &Identifier,
    slot: &CallbackSlot,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Option<String>> {
    let Some(CBridgeType::FunctionPointer { returns, params }) =
        slot.parameters().iter().find_map(|parameter| {
            matches!(parameter.ty(), CBridgeType::FunctionPointer { .. }).then(|| parameter.ty())
        })
    else {
        return Ok(None);
    };
    let params = params
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            Ok(format!(
                "{}{} arg{index}",
                bool_parameter_attribute(ty),
                c_type(ty, bridge, context)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    Ok(Some(format!(
        "        [global::System.Runtime.InteropServices.UnmanagedFunctionPointer(global::System.Runtime.InteropServices.CallingConvention.Cdecl)]\n        internal delegate {} {method}Completion({params});",
        c_type(returns, bridge, context)?
    )))
}

fn bool_parameter_attribute(ty: &CBridgeType) -> &'static str {
    match ty {
        CBridgeType::Bool => {
            "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)] "
        }
        _ => "",
    }
}

fn render_async_entry_body(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    method_name: &Identifier,
    parameters: &LoweredParameters,
    slot: &CallbackSlot,
    bridge: &CBridgeContract,
    context: &RenderContext<Native>,
) -> Result<Statement> {
    let Some(ParameterGroup::CallbackCompletion(completion)) = slot.parameter_groups().last()
    else {
        return broken_callback("async callback completion group");
    };
    let callback = Identifier::escape(slot.parameter(completion.callback()).name())?;
    let completion_context = Identifier::escape(slot.parameter(completion.context()).name())?;
    let CBridgeType::FunctionPointer { params, .. } = slot.parameter(completion.callback()).ty()
    else {
        return broken_callback("async callback completion function pointer");
    };
    if params.len() < 2 || params[1] != CBridgeType::Status {
        return broken_callback("async callback completion signature");
    }
    let payload = (params.len() == 3).then_some("boltffiPayload");
    if params.len() > 3 {
        return broken_callback("async callback completion payload count");
    }
    let arguments = parameters.entry_arguments.join(", ");
    let call = format!("await implementation.{method_name}({arguments}).ConfigureAwait(false)");
    let mut success = vec![
        "if (!Handles.TryGetValue(handle, out var implementation))\n{\n    boltffiComplete(100, default);\n    return;\n}"
            .to_owned(),
    ];
    success.extend(parameters.entry_setup.iter().cloned());
    match declaration.callable().returns().plan() {
        ReturnPlan::Void => {
            success.push(format!("{call};"));
            success.push("boltffiComplete(0, default);".to_owned());
        }
        ReturnPlan::DirectViaReturnSlot { .. } | ReturnPlan::DirectViaOutPointer { .. } => {
            success.push(format!("var boltffiValue = {call};"));
            success.push("boltffiComplete(0, boltffiValue);".to_owned());
        }
        ReturnPlan::EncodedViaReturnSlot { codec, .. }
        | ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let writes = codec
                .render_with(&mut Writer::new(
                    Identifier::parse("boltffiSuccessWriter")?,
                    Expression::identifier(Identifier::parse("boltffiValue")?),
                    context,
                ))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            success.push(format!("var boltffiValue = {call};"));
            success.push("WireWriter boltffiSuccessWriter = new WireWriter();".to_owned());
            success.extend(writes.into_iter().map(|statement| statement.to_string()));
            success.push(
                "boltffiComplete(0, FfiBuf.FromBytes(boltffiSuccessWriter.ToArray()));".to_owned(),
            );
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            success.push(format!("var boltffiValue = {call};"));
            success.push("WireWriter boltffiSuccessWriter = new WireWriter();".to_owned());
            success.push(format!(
                "if (boltffiValue.HasValue)\n{{\n    boltffiSuccessWriter.WriteU8(1);\n    boltffiSuccessWriter.{}(boltffiValue.Value);\n}}\nelse\n{{\n    boltffiSuccessWriter.WriteU8(0);\n}}",
                primitive_write_method(*primitive)
            ));
            success.push(
                "boltffiComplete(0, FfiBuf.FromBytes(boltffiSuccessWriter.ToArray()));".to_owned(),
            );
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            success.push(format!("var boltffiValue = {call};"));
            success.push(match element {
                DirectVectorElementType::Primitive(primitive)
                    if primitive.primitive() == Primitive::Bool =>
                {
                    "boltffiComplete(0, FfiBuf.FromRawBoolArray(boltffiValue));".to_owned()
                }
                _ => "boltffiComplete(0, FfiBuf.FromRawArray(boltffiValue));".to_owned(),
            });
        }
        ReturnPlan::HandleViaReturnSlot { target, .. }
        | ReturnPlan::HandleViaOutPointer { target, .. } => {
            success.push(format!("var boltffiValue = {call};"));
            success.push(match target {
                HandleTarget::Class(_) => "boltffiComplete(0, boltffiValue.Handle);".to_owned(),
                HandleTarget::Callback(callback) => {
                    let ty = type_name::callback(*callback, context)?;
                    format!("boltffiComplete(0, {ty}Bridge.Create(boltffiValue));")
                }
                _ => return super::unsupported("async stream callback return"),
            });
        }
        _ => return super::unsupported("async callback return shape"),
    }

    let mut catches = Vec::new();
    let mut catches_all_exceptions = false;
    if let ErrorChannel::Encoded {
        placement: ErrorPlacement::ReturnSlot,
        ty,
        codec,
        shape: native::BufferShape::Buffer,
    } = declaration.callable().error().channel()
    {
        catches_all_exceptions = matches!(ty, TypeRef::String);
        let error = Identifier::parse("boltffiError")?;
        let writer = Identifier::parse("boltffiErrorWriter")?;
        let (exception, value) = callback_error_exception(ty, &error, context)?;
        let writes = codec
            .render_with(&mut Writer::new(writer.clone(), value, context))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let mut body = vec![format!("WireWriter {writer} = new WireWriter();")];
        body.extend(writes.into_iter().map(|statement| statement.to_string()));
        body.push(format!(
            "boltffiComplete(1, FfiBuf.FromBytes({writer}.ToArray()));"
        ));
        catches.push(format!(
            "catch ({exception} {error})\n{{\n{}\n}}",
            indent_lines(&body, 4)
        ));
    }
    if !catches_all_exceptions {
        catches.push("catch\n{\n    boltffiComplete(100, default);\n}".to_owned());
    }
    let complete_values = |status: &str, value: &str| {
        params
            .iter()
            .enumerate()
            .map(|(index, _)| match index {
                0 => completion_context.to_string(),
                1 => format!("new FfiStatus {{ code = {status} }}"),
                _ => value.to_owned(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let local_payload = payload.unwrap_or("_");
    let body = vec![
        format!(
            "void boltffiComplete(int boltffiStatus, {} {local_payload}) => {callback}({});",
            payload
                .map(|_| c_type(&params[2], bridge, context))
                .transpose()?
                .unwrap_or_else(|| TypeFragment::new("object?")),
            complete_values("boltffiStatus", local_payload),
        ),
        format!(
            "try\n{{\n{}\n}}\n{}",
            indent_lines(&success, 4),
            catches.join("\n")
        ),
    ];
    Ok(Statement::new(indent_lines(&body, 12)))
}

fn render_async_proxy_body(
    declaration: &ImportedMethodDecl<Native, VTableSlot>,
    method_name: &Identifier,
    bridge_name: &Identifier,
    parameters: &LoweredParameters,
    slot: &CallbackSlot,
    context: &RenderContext<Native>,
) -> Result<Statement> {
    let Some(ParameterGroup::CallbackCompletion(completion_group)) = slot.parameter_groups().last()
    else {
        return broken_callback("async callback completion group");
    };
    let CBridgeType::FunctionPointer { params, .. } =
        slot.parameter(completion_group.callback()).ty()
    else {
        return broken_callback("async callback completion function pointer");
    };
    if !(2..=3).contains(&params.len()) || params[1] != CBridgeType::Status {
        return broken_callback("async callback completion signature");
    }
    let has_payload = params.len() == 3;
    let returns_void = matches!(declaration.callable().returns().plan(), ReturnPlan::Void);
    let task_type = match returns_void {
        true => TypeFragment::new("bool"),
        false => public_return_type(declaration.callable().returns().plan(), context)?,
    };
    let slot_name = Name::new(declaration.name()).snake();
    let mut body = vec![
        "if (handle.IsNull) throw new global::System.ObjectDisposedException(GetType().Name);"
            .to_owned(),
        format!(
            "{bridge_name}.{method_name}Fn invoke = global::System.Runtime.InteropServices.Marshal.GetDelegateForFunctionPointer<{bridge_name}.{method_name}Fn>(vtable.{slot_name});"
        ),
    ];
    body.extend(parameters.proxy_setup.iter().cloned());
    body.push(format!(
        "var boltffiCompletionSource = new global::System.Threading.Tasks.TaskCompletionSource<{task_type}>(global::System.Threading.Tasks.TaskCreationOptions.RunContinuationsAsynchronously);"
    ));
    body.push(
        "global::System.Runtime.InteropServices.GCHandle boltffiCompletionHandle = default;"
            .to_owned(),
    );
    let completion_parameters = match has_payload {
        true => "(boltffiContext, boltffiStatus, boltffiPayload)",
        false => "(boltffiContext, boltffiStatus)",
    };
    let mut success = Vec::new();
    match declaration.callable().returns().plan() {
        ReturnPlan::Void => success.push("boltffiCompletionSource.TrySetResult(true);".to_owned()),
        ReturnPlan::DirectViaReturnSlot { .. } | ReturnPlan::DirectViaOutPointer { .. } => {
            success.push("boltffiCompletionSource.TrySetResult(boltffiPayload);".to_owned())
        }
        ReturnPlan::EncodedViaReturnSlot { codec, .. }
        | ReturnPlan::EncodedViaOutPointer { codec, .. } => {
            let reader = Identifier::parse("boltffiSuccessReader")?;
            let decode = codec
                .read_plan()
                .render_with(&mut Reader::new(reader.clone(), context))
                .map(ReadExpression::into_expression)?;
            success.push(format!(
                "WireReader {reader} = new WireReader(boltffiPayload);"
            ));
            success.push(format!("boltffiCompletionSource.TrySetResult({decode});"));
        }
        ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
            success.push(
                "WireReader boltffiSuccessReader = new WireReader(boltffiPayload);".to_owned(),
            );
            success.push(format!(
                "boltffiCompletionSource.TrySetResult(boltffiSuccessReader.ReadU8() == 0 ? default({}?) : boltffiSuccessReader.{}());",
                primitive_type(*primitive),
                primitive_read_method(*primitive)
            ));
        }
        ReturnPlan::DirectVecViaReturnSlot { element } => {
            let element_type = direct_vector_element_type(element, None, context)?;
            success.push(
                "WireReader boltffiSuccessReader = new WireReader(boltffiPayload);".to_owned(),
            );
            success.push(format!(
                "boltffiCompletionSource.TrySetResult({});",
                match element {
                    DirectVectorElementType::Primitive(primitive)
                        if primitive.primitive() == Primitive::Bool =>
                    {
                        "boltffiSuccessReader.ReadRawBoolArray()".to_owned()
                    }
                    _ => format!("boltffiSuccessReader.ReadRawArray<{element_type}>()"),
                }
            ));
        }
        ReturnPlan::HandleViaReturnSlot {
            target, presence, ..
        }
        | ReturnPlan::HandleViaOutPointer {
            target, presence, ..
        } => {
            let value = match target {
                HandleTarget::Class(class) => {
                    let ty = type_name::class(*class, context)?;
                    match presence {
                        HandlePresence::Required => format!("new {ty}(boltffiPayload)"),
                        HandlePresence::Nullable => {
                            format!("boltffiPayload == 0 ? null : new {ty}(boltffiPayload)")
                        }
                        _ => return super::unsupported("async callback handle presence"),
                    }
                }
                HandleTarget::Callback(callback) => {
                    let ty = type_name::callback(*callback, context)?;
                    match presence {
                        HandlePresence::Required => {
                            format!("{ty}Bridge.Wrap(boltffiPayload)")
                        }
                        HandlePresence::Nullable => format!(
                            "boltffiPayload.IsNull ? null : {ty}Bridge.Wrap(boltffiPayload)"
                        ),
                        _ => return super::unsupported("async callback handle presence"),
                    }
                }
                _ => return super::unsupported("async stream callback return"),
            };
            success.push(format!("boltffiCompletionSource.TrySetResult({value});"));
        }
        _ => return super::unsupported("async callback proxy return shape"),
    }

    let mut status = vec![format!(
        "if (boltffiStatus.code == 0)\n{{\n{}\n}}",
        indent_lines(&success, 4)
    )];
    if let ErrorChannel::Encoded {
        placement: ErrorPlacement::ReturnSlot,
        ty,
        codec,
        shape: native::BufferShape::Buffer,
    } = declaration.callable().error().channel()
    {
        let reader = Identifier::parse("boltffiErrorReader")?;
        let decode = codec
            .read_plan()
            .render_with(&mut Reader::new(reader.clone(), context))
            .map(ReadExpression::into_expression)?;
        let exception = callback_error_throw(ty, decode, context)?;
        status.push(format!(
            "else if (boltffiStatus.code == 1)\n{{\n    WireReader {reader} = new WireReader(boltffiPayload);\n    boltffiCompletionSource.TrySetException({exception});\n}}"
        ));
    }
    status.push(
        "else\n{\n    boltffiCompletionSource.TrySetException(new global::System.InvalidOperationException($\"callback failed with status code {boltffiStatus.code}\"));\n}"
            .to_owned(),
    );
    let payload_is_buffer = has_payload && params[2] == CBridgeType::Buffer;
    body.push(format!(
        "{bridge_name}.{method_name}Completion boltffiCompletion = {completion_parameters} =>\n{{\n    try\n    {{\n{}\n    }}\n    catch (global::System.Exception boltffiException)\n    {{\n        boltffiCompletionSource.TrySetException(boltffiException);\n    }}\n    finally\n    {{\n{}        if (boltffiCompletionHandle.IsAllocated) boltffiCompletionHandle.Free();\n    }}\n}};",
        indent_lines(&status, 8),
        if payload_is_buffer {
            "        if (boltffiPayload.ptr != 0) NativeMethods.FreeBuf(boltffiPayload);\n"
        } else {
            ""
        },
    ));
    body.push(
        "boltffiCompletionHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(boltffiCompletion);"
            .to_owned(),
    );

    let mut arguments = vec![None; slot.parameters().len()];
    arguments[0] = Some("handle.handle".to_owned());
    let mut source_arguments = parameters.proxy_arguments.iter();
    for group in slot.source_parameter_groups() {
        for index in callback_group_indices(group)? {
            arguments[index] = Some(
                source_arguments
                    .next()
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: "c",
                        invariant: "async callback proxy source argument is missing",
                    })?
                    .clone(),
            );
        }
    }
    arguments[completion_group.callback().position()] = Some("boltffiCompletion".to_owned());
    arguments[completion_group.context().position()] = Some(
        "global::System.Runtime.InteropServices.GCHandle.ToIntPtr(boltffiCompletionHandle)"
            .to_owned(),
    );
    let arguments =
        arguments
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "async callback proxy native argument is missing",
            })?;
    let invoke = format!("invoke({});", arguments.join(", "));
    let call = if parameters.proxy_cleanup.is_empty() {
        invoke
    } else {
        format!(
            "try\n{{\n{}\n}}\nfinally\n{{\n{}\n}}",
            indent_lines(&[invoke], 4),
            indent_lines(&parameters.proxy_cleanup, 4),
        )
    };
    body.push(format!(
        "try\n{{\n{}\n}}\ncatch (global::System.Exception boltffiException)\n{{\n    if (boltffiCompletionHandle.IsAllocated) boltffiCompletionHandle.Free();\n    boltffiCompletionSource.TrySetException(boltffiException);\n}}",
        indent_lines(&[call], 4)
    ));
    body.push("return boltffiCompletionSource.Task;".to_owned());
    Ok(Statement::new(indent_lines(&body, 12)))
}

fn unsupported_body(slot: &CallbackSlot) -> Result<Statement> {
    let mut body = slot
        .parameters()
        .iter()
        .filter(|parameter| {
            matches!(
                parameter.ty(),
                CBridgeType::MutPointer(inner) if inner.as_ref() != &CBridgeType::Void
            )
        })
        .map(|parameter| {
            Identifier::escape(parameter.name()).map(|name| format!("{name} = default;"))
        })
        .collect::<Result<Vec<_>>>()?;
    body.push(match slot.returns() {
        CBridgeType::Void => "return;".to_owned(),
        _ => "return default;".to_owned(),
    });
    Ok(Statement::new(indent_lines(&body, 12)))
}

fn broken_callback<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}

fn indent_lines(lines: &[String], spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    lines
        .iter()
        .flat_map(|line| line.lines())
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
