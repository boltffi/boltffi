use std::collections::BTreeMap;

use askama::Template;
use boltffi_binding::{
    ErrorChannel, ExecutionDecl, FunctionDecl, IncomingParam, Native, ParamPlan, Primitive,
    ReturnPlan,
};

use crate::{
    bridge::c::{CBridgeContract, Function as CFunction, ParameterGroup, Type as CBridgeType},
    core::{
        AuxChunk, Diagnostic, Emitted, Error, FilePath, GeneratedFile, GeneratedOutput, HelperId,
        RenderedDeclaration, Result,
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
pub(super) struct Function {
    name: Identifier,
    native_name: Identifier,
    parameters: Vec<Parameter>,
    public_return_type: TypeFragment,
    native_return_type: TypeFragment,
    return_marshal_i1: bool,
    checks_status: bool,
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
    ) -> Result<Self> {
        let c_function = bridge_function(declaration, bridge)?;
        let callable = declaration.callable();

        if !matches!(callable.execution(), ExecutionDecl::Synchronous(_)) {
            return unsupported("asynchronous functions");
        }
        if !matches!(callable.error().channel(), ErrorChannel::None) {
            return unsupported("fallible functions");
        }
        if callable.params().len() != c_function.parameter_groups().len() {
            return broken_contract("function parameter group count does not match the C bridge");
        }

        let parameters = callable
            .params()
            .iter()
            .zip(c_function.parameter_groups())
            .map(|(parameter, c_parameter)| {
                let primitive = match parameter.payload() {
                    IncomingParam::Value(ParamPlan::Direct {
                        ty: boltffi_binding::DirectValueType::Primitive(primitive),
                        ..
                    }) => *primitive,
                    _ => return unsupported("non-primitive function parameters"),
                };
                let ParameterGroup::Value(index) = c_parameter else {
                    return broken_contract(
                        "primitive function parameter does not use a C value group",
                    );
                };
                let c_parameter = c_function.parameter(*index);
                let expected = CBridgeType::primitive(primitive)?;
                if c_parameter.ty() != &expected {
                    return broken_contract("function parameter type does not match the C bridge");
                }
                Ok(Parameter {
                    name: Name::new(parameter.name()).camel()?,
                    ty: primitive_type(primitive),
                    marshal_i1: matches!(primitive, Primitive::Bool),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let (
            public_return_type,
            native_return_type,
            return_marshal_i1,
            checks_status,
            expected_return,
        ) = match callable.returns().plan() {
            ReturnPlan::Void => (
                TypeFragment::void(),
                TypeFragment::new("FfiStatus"),
                false,
                true,
                CBridgeType::Status,
            ),
            ReturnPlan::DirectViaReturnSlot {
                ty: boltffi_binding::DirectValueType::Primitive(primitive),
            } => (
                primitive_type(*primitive),
                primitive_type(*primitive),
                matches!(primitive, Primitive::Bool),
                false,
                CBridgeType::primitive(*primitive)?,
            ),
            _ => return unsupported("non-primitive function returns"),
        };
        if c_function.returns() != &expected_return {
            return broken_contract("function return type does not match the C bridge");
        }

        let name = Name::new(declaration.name()).pascal()?;
        let native_name = Identifier::parse(format!("Native{name}"))?;
        let invocation = Expression::call(
            Expression::member(Identifier::parse("NativeMethods")?, native_name.clone()),
            ArgumentList::new(
                parameters
                    .iter()
                    .map(|parameter| Expression::identifier(parameter.name.clone())),
            ),
        );

        Ok(Self {
            name,
            native_name,
            parameters,
            public_return_type,
            native_return_type,
            return_marshal_i1,
            checks_status,
            invocation,
            entry_point: Literal::string(c_function.name()),
            helper_id: HelperId::new(declaration.name().clone()),
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

        for declaration in declarations {
            let (_, emitted) = declaration.into_parts();
            let (primary, aux, emitted_diagnostics) = emitted.into_parts();
            diagnostics.extend(emitted_diagnostics);
            if !primary.is_empty() {
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
        Ok(GeneratedOutput::new(
            vec![GeneratedFile::new(path, source)],
            diagnostics,
        ))
    }
}

fn bridge_function<'bridge>(
    declaration: &FunctionDecl<Native>,
    bridge: &'bridge CBridgeContract,
) -> Result<&'bridge CFunction> {
    let symbol = declaration.symbol().name().as_str();
    bridge
        .functions()
        .iter()
        .find(|function| function.name() == symbol)
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "function symbol is missing from the C bridge",
        })
}

fn primitive_type(primitive: Primitive) -> TypeFragment {
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
