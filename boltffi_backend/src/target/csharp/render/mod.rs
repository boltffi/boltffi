mod enumeration;
mod record;

pub(in crate::target::csharp) use enumeration::Enumeration;
pub(in crate::target::csharp) use record::Record;

use std::collections::BTreeMap;

use askama::Template;
use boltffi_binding::{
    CanonicalName, DeclarationRef, DirectValueType, ErrorChannel, ExecutionDecl, ExportedCallable,
    ExportedMethodDecl, FunctionDecl, IncomingParam, InitializerDecl, Native, NativeSymbol,
    ParamPlan, Primitive, Receive, ReturnPlan,
};

use crate::{
    bridge::c::{CBridgeContract, Function as CFunction, ParameterGroup, Type as CBridgeType},
    core::{
        AuxChunk, Diagnostic, Emitted, Error, FilePath, GeneratedFile, GeneratedOutput, HelperId,
        RenderContext, RenderedDeclaration, Result,
    },
};

use super::{
    name_style::{Name, Namespace},
    syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeFragment},
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
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_function = bridge_function(symbol, bridge)?;

        if !matches!(callable.execution(), ExecutionDecl::Synchronous(_)) {
            return unsupported("asynchronous functions");
        }
        if !matches!(callable.error().channel(), ErrorChannel::None) {
            return unsupported("fallible functions");
        }
        let mut parameter_groups = c_function.parameter_groups().iter();
        let mut native_parameters = Vec::new();
        let mut invocation_arguments = Vec::new();
        let mut return_after_status = None;

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
            let receiver = lower_receiver(
                owner, owner_name, receive, extension, group, c_function, bridge,
            )?;
            native_parameters.extend(receiver.native_parameters);
            invocation_arguments.extend(receiver.arguments);
            return_after_status = receiver.return_after_status;
        }

        let mut parameters = Vec::new();
        for parameter in callable.params() {
            let (ty, receive) = match parameter.payload() {
                IncomingParam::Value(ParamPlan::Direct { ty, receive }) => (ty, *receive),
                _ => return unsupported("non-primitive function parameters"),
            };
            let group = parameter_groups.next().ok_or(Error::BrokenBridgeContract {
                bridge: "c",
                invariant: "function parameter is missing from the C bridge",
            })?;
            let ParameterGroup::Value(index) = group else {
                return unsupported("mutable direct function parameters");
            };
            let c_parameter = c_function.parameter(*index);
            let modifier = direct_parameter_modifier(ty, receive, c_parameter.ty(), bridge)?;
            let name = Name::new(parameter.name()).camel()?;
            let rendered_type = direct_type(ty, context)?;
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
            });
            invocation_arguments.push(Expression::identifier(name));
        }

        if parameter_groups.next().is_some() {
            return broken_contract("function parameter group count does not match the C bridge");
        }

        let (public_return_type, native_return_type, return_marshal_i1, checks_status) =
            match callable.returns().plan() {
                ReturnPlan::Void => {
                    if c_function.returns() != &CBridgeType::Status {
                        return broken_contract("void return type does not match the C bridge");
                    }
                    let public_return_type = match (&return_after_status, &call_site) {
                        (Some(_), CallSite::Record { owner, .. }) => direct_type(owner, context)?,
                        (Some(_), _) => return unsupported("mutable enum receiver"),
                        (None, _) => TypeFragment::void(),
                    };
                    (
                        public_return_type,
                        TypeFragment::new("FfiStatus"),
                        false,
                        true,
                    )
                }
                ReturnPlan::DirectViaReturnSlot { ty } => {
                    if return_after_status.is_some() {
                        return unsupported("mutable direct record method returns");
                    }
                    if !c_direct_matches(ty, c_function.returns(), bridge)? {
                        return broken_contract("function return type does not match the C bridge");
                    }
                    let rendered = direct_type(ty, context)?;
                    (
                        rendered.clone(),
                        rendered,
                        matches!(ty, DirectValueType::Primitive(Primitive::Bool)),
                        false,
                    )
                }
                _ => return unsupported("non-primitive function returns"),
            };

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
        match self.checks_status {
            true => Ok(emitted.with_aux(AuxChunk::ForwardDecl(StatusTemplate.render()?.into()))),
            false => Ok(emitted),
        }
    }
}

struct LoweredReceiver {
    native_parameters: Vec<NativeParameter>,
    arguments: Vec<Expression>,
    return_after_status: Option<Expression>,
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
                    },
                    NativeParameter {
                        name: output_name.clone(),
                        ty: ty.clone(),
                        modifier: "out ",
                        marshal_i1: false,
                    },
                ],
                arguments: vec![
                    receiver_expression,
                    Expression::new(format!("out {ty} {output_name}")),
                ],
                return_after_status: Some(Expression::identifier(output_name)),
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
                }],
                arguments: vec![receiver_expression],
                return_after_status: None,
            })
        }
        (DirectValueType::Enum(_), Receive::ByMutRef, _) => unsupported("mutable enum receiver"),
        _ => broken_contract("method receiver does not match the C bridge"),
    }
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

pub(super) fn primitive_type(primitive: Primitive) -> TypeFragment {
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
