use boltffi_binding::{
    CanonicalName, ClosureParameter, DirectValueType, DirectVectorElementType, ErrorChannel,
    ErrorPlacement, Native, OutgoingParam, ParamPlan, Primitive, ReturnPlan, TypeRef, native,
};

use crate::{
    bridge::c::{ClosureParameter as CClosureParameter, ParameterGroup},
    core::{Error, HelperId, RenderContext, Result},
};

use super::super::{
    codec::{ReadExpression, Reader, Writer},
    syntax::{Expression, Identifier, Statement, TypeFragment},
    type_name,
};
use super::{NativeParameter, Parameter, direct_type, direct_vector_element_type};

pub(super) struct ClosureArgument {
    pub(super) parameter: Parameter,
    pub(super) native_parameters: Vec<NativeParameter>,
    pub(super) invocation_arguments: Vec<Expression>,
    pub(super) setup: Statement,
    pub(super) helper: ClosureHelper,
    pub(super) requires_wire_runtime: bool,
    pub(super) requires_copy_buffer: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ClosureHelper {
    pub(super) id: HelperId,
    pub(super) source: Statement,
}

impl ClosureArgument {
    pub(super) fn from_declaration(
        name: Identifier,
        helper_name: Identifier,
        declaration: &ClosureParameter<Native, boltffi_binding::IntoRust>,
        c_closure: &CClosureParameter,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let all_groups = c_closure.parameter_groups();
        if declaration.invoke().params().len() > all_groups.len() {
            return broken("closure source parameter count");
        }
        let (groups, return_groups) = all_groups.split_at(declaration.invoke().params().len());

        let mut public_parameters = Vec::new();
        let mut native_parameters = vec!["nint context".to_owned()];
        let mut invocation_arguments = Vec::new();
        let mut entry_setup = Vec::new();
        let mut requires_wire_runtime = false;
        for (parameter, group) in declaration.invoke().params().iter().zip(groups) {
            let OutgoingParam::Value(plan) = parameter.payload() else {
                return super::super::unsupported("nested closure parameter");
            };
            match plan {
                ParamPlan::Direct { ty, .. } => {
                    let ParameterGroup::Value(index) = group else {
                        return broken("direct closure parameter group");
                    };
                    let parameter_name = Identifier::escape(c_closure.parameter(*index).name())?;
                    let ty = direct_type(ty, context)?;
                    public_parameters.push(ty.clone());
                    native_parameters.push(format!(
                        "{}{} {parameter_name}",
                        if ty.to_string() == "bool" {
                            "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)] "
                        } else {
                            ""
                        },
                        ty,
                    ));
                    invocation_arguments.push(parameter_name.to_string());
                }
                ParamPlan::Encoded { ty, codec, .. } => {
                    let ParameterGroup::ByteSlice(slice) = group else {
                        return broken("encoded closure parameter group");
                    };
                    requires_wire_runtime = true;
                    let pointer = Identifier::escape(c_closure.parameter(slice.pointer()).name())?;
                    let length = Identifier::escape(c_closure.parameter(slice.length()).name())?;
                    let reader = Identifier::parse(format!("boltffi{pointer}Reader"))?;
                    let decode = codec
                        .render_with(&mut Reader::new(reader.clone(), context))
                        .map(ReadExpression::into_expression)?;
                    public_parameters.push(type_name::type_ref(ty, context)?);
                    native_parameters
                        .extend([format!("nint {pointer}"), format!("nuint {length}")]);
                    entry_setup.push(format!(
                        "WireReader {reader} = new WireReader({pointer}, {length});"
                    ));
                    invocation_arguments.push(decode.to_string());
                }
                ParamPlan::DirectVec { element, .. } => {
                    let ParameterGroup::DirectVector(vector) = group else {
                        return broken("direct-vector closure parameter group");
                    };
                    requires_wire_runtime = true;
                    let pointer = Identifier::escape(c_closure.parameter(vector.pointer()).name())?;
                    let length = Identifier::escape(c_closure.parameter(vector.length()).name())?;
                    let element_type = direct_vector_element_type(element, None, context)?;
                    let reader = Identifier::parse(format!("boltffi{pointer}Reader"))?;
                    public_parameters.push(TypeFragment::new(format!("{element_type}[]")));
                    native_parameters
                        .extend([format!("nint {pointer}"), format!("nuint {length}")]);
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
                        _ => {
                            return super::super::unsupported("closure direct-vector element type");
                        }
                    };
                    entry_setup.push(format!(
                        "WireReader {reader} = new WireReader({pointer}, {byte_length});"
                    ));
                    invocation_arguments.push(match element {
                        DirectVectorElementType::Primitive(primitive)
                            if primitive.primitive() == Primitive::Bool =>
                        {
                            format!("{reader}.ReadRawBoolArray()")
                        }
                        _ => format!("{reader}.ReadRawArray<{element_type}>()"),
                    });
                }
                _ => return super::super::unsupported("closure parameter shape"),
            }
        }

        let (public_return, native_return, call, requires_copy_buffer) = match declaration
            .invoke()
            .returns()
            .plan()
        {
            ReturnPlan::Void => (
                None,
                TypeFragment::void(),
                format!("implementation({});", invocation_arguments.join(", ")),
                false,
            ),
            ReturnPlan::DirectViaReturnSlot { ty } => {
                let ty = direct_type(ty, context)?;
                (
                    Some(ty.clone()),
                    ty,
                    format!(
                        "return implementation({});",
                        invocation_arguments.join(", ")
                    ),
                    false,
                )
            }
            ReturnPlan::EncodedViaReturnSlot { ty, codec, .. } => {
                requires_wire_runtime = true;
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
                let mut call = vec![format!(
                    "var {value} = implementation({});",
                    invocation_arguments.join(", ")
                )];
                call.push(format!("WireWriter {writer} = new WireWriter();"));
                call.extend(writes.into_iter().map(|statement| statement.to_string()));
                call.push(format!("return FfiBuf.FromBytes({writer}.ToArray());"));
                (
                    Some(type_name::type_ref(ty, context)?),
                    TypeFragment::new("FfiBuf"),
                    call.join("\n            "),
                    true,
                )
            }
            ReturnPlan::DirectViaOutPointer { ty } => {
                let ErrorChannel::Encoded {
                    placement: ErrorPlacement::ReturnSlot,
                    ty: error_type,
                    codec: error_codec,
                    shape: native::BufferShape::Buffer,
                } = declaration.invoke().error().channel()
                else {
                    return super::super::unsupported("fallible closure error channel");
                };
                let [ParameterGroup::SuccessOut(index)] = return_groups else {
                    return broken("fallible closure success out parameter");
                };
                requires_wire_runtime = true;
                let success_name = Identifier::escape(c_closure.parameter(*index).name())?;
                let success_type = direct_type(ty, context)?;
                native_parameters.push(format!(
                    "{}out {success_type} {success_name}",
                    if matches!(ty, DirectValueType::Primitive(Primitive::Bool)) {
                        "[global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)] "
                    } else {
                        ""
                    }
                ));
                let error = Identifier::parse("boltffiError")?;
                let writer = Identifier::parse("boltffiErrorWriter")?;
                let (exception, error_value) = error_exception(error_type, &error, context)?;
                let writes = error_codec
                    .render_with(&mut Writer::new(writer.clone(), error_value, context))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;
                let mut failure = vec![format!("WireWriter {writer} = new WireWriter();")];
                failure.extend(writes.into_iter().map(|statement| statement.to_string()));
                failure.push(format!("return FfiBuf.FromBytes({writer}.ToArray());"));
                let call = format!(
                    "{success_name} = default;\n            try\n            {{\n                {success_name} = implementation({});\n                return default;\n            }}\n            catch ({exception} {error})\n            {{\n{}\n            }}",
                    invocation_arguments.join(", "),
                    failure
                        .iter()
                        .map(|line| format!("                {line}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                (Some(success_type), TypeFragment::new("FfiBuf"), call, true)
            }
            _ => return super::super::unsupported("closure return shape"),
        };

        let public_type = match &public_return {
            Some(return_type) => TypeFragment::new(format!(
                "global::System.Func<{}>",
                public_parameters
                    .iter()
                    .chain(std::iter::once(return_type))
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            None if public_parameters.is_empty() => TypeFragment::new("global::System.Action"),
            None => TypeFragment::new(format!(
                "global::System.Action<{}>",
                public_parameters
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        };
        let call_delegate = Identifier::parse(format!("{helper_name}Call"))?;
        let release_delegate = Identifier::parse(format!("{helper_name}Release"))?;
        let invoke = Identifier::parse(format!("{helper_name}Invoke"))?;
        let release = Identifier::parse(format!("{helper_name}Drop"))?;
        let handle = Identifier::parse(format!("boltffi{name}Handle"))?;
        let return_attribute = matches!(native_return.to_string().as_str(), "bool")
            .then_some("        [return: global::System.Runtime.InteropServices.MarshalAs(global::System.Runtime.InteropServices.UnmanagedType.I1)]\n")
            .unwrap_or("");
        let source = format!(
            "        [global::System.Runtime.InteropServices.UnmanagedFunctionPointer(global::System.Runtime.InteropServices.CallingConvention.Cdecl)]\n{return_attribute}        internal delegate {native_return} {call_delegate}({});\n        internal static readonly {call_delegate} {call_delegate}Instance = {invoke};\n\n        [global::System.Runtime.InteropServices.UnmanagedFunctionPointer(global::System.Runtime.InteropServices.CallingConvention.Cdecl)]\n        internal delegate void {release_delegate}(nint context);\n        internal static readonly {release_delegate} {release_delegate}Instance = {release};\n\n        private static {native_return} {invoke}({})\n        {{\n            var implementation = ({public_type})global::System.Runtime.InteropServices.GCHandle.FromIntPtr(context).Target!;\n{}            {call}\n        }}\n\n        private static void {release}(nint context)\n        {{\n            if (context != 0) global::System.Runtime.InteropServices.GCHandle.FromIntPtr(context).Free();\n        }}",
            native_parameters.join(", "),
            native_parameters.join(", "),
            entry_setup
                .iter()
                .map(|line| format!("            {line}\n"))
                .collect::<String>(),
        );
        Ok(Self {
            parameter: Parameter {
                name: name.clone(),
                ty: public_type,
                marshal_i1: false,
            },
            native_parameters: vec![
                NativeParameter {
                    name: Identifier::parse(format!("{name}Call"))?,
                    ty: TypeFragment::new(format!("NativeMethods.{call_delegate}")),
                    modifier: "",
                    marshal_i1: false,
                    marshal_bool_array: false,
                    array_out: false,
                    byte_array: false,
                },
                NativeParameter {
                    name: Identifier::parse(format!("{name}Context"))?,
                    ty: TypeFragment::new("nint"),
                    modifier: "",
                    marshal_i1: false,
                    marshal_bool_array: false,
                    array_out: false,
                    byte_array: false,
                },
                NativeParameter {
                    name: Identifier::parse(format!("{name}Release"))?,
                    ty: TypeFragment::new(format!("NativeMethods.{release_delegate}")),
                    modifier: "",
                    marshal_i1: false,
                    marshal_bool_array: false,
                    array_out: false,
                    byte_array: false,
                },
            ],
            invocation_arguments: vec![
                Expression::new(format!("NativeMethods.{call_delegate}Instance")),
                Expression::new(format!(
                    "global::System.Runtime.InteropServices.GCHandle.ToIntPtr({handle})"
                )),
                Expression::new(format!("NativeMethods.{release_delegate}Instance")),
            ],
            setup: Statement::new(format!(
                "global::System.Runtime.InteropServices.GCHandle {handle} = global::System.Runtime.InteropServices.GCHandle.Alloc({name});"
            )),
            helper: ClosureHelper {
                id: HelperId::new(CanonicalName::single(helper_name.as_str())),
                source: Statement::new(source),
            },
            requires_wire_runtime,
            requires_copy_buffer,
        })
    }
}

fn broken<T>(invariant: &'static str) -> Result<T> {
    Err(Error::BrokenBridgeContract {
        bridge: "c",
        invariant,
    })
}

fn error_exception(
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
        _ => super::super::unsupported("fallible closure error type"),
    }
}
