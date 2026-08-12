use askama::Template;
use boltffi_binding::{Primitive, RecordId};

use crate::{
    bridge::c::{
        CBridgeContract, CallbackSlot, ClosureParameter, ClosureReturnParameter, Function,
        Parameter, ParameterIndex, Type,
    },
    core::{Error, Result},
};

pub fn direct_record_struct(bridge: &CBridgeContract, id: RecordId) -> Result<String> {
    let record = bridge
        .source_direct_record(id)
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "Dart direct record is missing from the C bridge",
        })?;
    Ok(format!("_$${}", record.name()))
}

pub trait NativeParameterSource {
    fn parameter(&self, index: ParameterIndex) -> &Parameter;
}

pub trait NativeCallableSource: NativeParameterSource {
    fn returns(&self) -> &Type;
}

impl NativeParameterSource for Function {
    fn parameter(&self, index: ParameterIndex) -> &Parameter {
        Function::parameter(self, index)
    }
}

impl NativeCallableSource for Function {
    fn returns(&self) -> &Type {
        Function::returns(self)
    }
}

impl NativeParameterSource for CallbackSlot {
    fn parameter(&self, index: ParameterIndex) -> &Parameter {
        CallbackSlot::parameter(self, index)
    }
}

impl NativeParameterSource for ClosureParameter {
    fn parameter(&self, index: ParameterIndex) -> &Parameter {
        ClosureParameter::parameter(self, index)
    }
}

impl NativeParameterSource for ClosureReturnParameter {
    fn parameter(&self, index: ParameterIndex) -> &Parameter {
        ClosureReturnParameter::parameter(self, index)
    }
}

impl NativeCallableSource for ClosureReturnParameter {
    fn returns(&self) -> &Type {
        match self.call_type() {
            Type::FunctionPointer { returns, .. } => returns,
            _ => unreachable!("C closure return call type was validated as a function pointer"),
        }
    }
}

use super::syntax::Identifier;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeType {
    native: String,
    dart: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunctionSignature {
    returns: NativeType,
    parameters: Vec<NativeType>,
}

#[derive(Template)]
#[template(path = "target/dart/native_function.dart", escape = "none")]
struct NativeFunctionTemplate<'a> {
    function: &'a NativeFunction,
}

struct NativeFunction {
    returns: NativeType,
    name: Identifier,
    parameters: Vec<NativeParameter>,
    leaf: bool,
}

struct NativeParameter {
    name: Identifier,
    ty: NativeType,
}

impl NativeFunctionSignature {
    pub fn from_pointer(ty: &Type) -> Result<Self> {
        let Type::FunctionPointer { returns, params } = ty else {
            return Err(Error::UnexpectedBindingShape {
                layer: "dart native function",
                shape: "expected a C function pointer",
            });
        };
        Ok(Self {
            returns: NativeType::from_c(returns)?,
            parameters: params
                .iter()
                .map(NativeType::from_c)
                .collect::<Result<Vec<_>>>()?,
        })
    }

    pub fn native(&self) -> String {
        format!(
            "{} Function({})",
            self.returns.native(),
            self.parameters
                .iter()
                .map(NativeType::native)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    pub fn dart(&self) -> String {
        format!(
            "{} Function({})",
            self.returns.dart(),
            self.parameters
                .iter()
                .map(NativeType::dart)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    pub fn returns(&self) -> &NativeType {
        &self.returns
    }

    pub fn parameters(&self) -> &[NativeType] {
        &self.parameters
    }
}

impl NativeType {
    pub fn from_c(ty: &Type) -> Result<Self> {
        match ty {
            Type::Void => Ok(Self::new("$$ffi.Void", "void")),
            Type::Bool => Ok(Self::new("$$ffi.Bool", "bool")),
            Type::Int8 => Ok(Self::integer("$$ffi.Int8")),
            Type::Uint8 => Ok(Self::integer("$$ffi.Uint8")),
            Type::Int16 => Ok(Self::integer("$$ffi.Int16")),
            Type::Uint16 => Ok(Self::integer("$$ffi.Uint16")),
            Type::Int32 => Ok(Self::integer("$$ffi.Int32")),
            Type::Uint32 => Ok(Self::integer("$$ffi.Uint32")),
            Type::Int64 => Ok(Self::integer("$$ffi.Int64")),
            Type::Uint64 => Ok(Self::integer("$$ffi.Uint64")),
            Type::Float32 => Ok(Self::floating("$$ffi.Float")),
            Type::Float64 => Ok(Self::floating("$$ffi.Double")),
            Type::SignedPointerWidth => Ok(Self::integer("$$ffi.IntPtr")),
            Type::PointerWidth => Ok(Self::integer("$$ffi.UintPtr")),
            Type::Status => Ok(Self::same("_$$BoltFFIStatus")),
            Type::Buffer => Ok(Self::same("_$$BoltFFIBuf")),
            Type::String => Ok(Self::same("_$$BoltFFIString")),
            Type::Span => Ok(Self::same("_$$BoltFFISpan")),
            Type::FutureHandle => Ok(Self::pointer(Self::new("$$ffi.Void", "void"))),
            Type::StreamPollResult => Ok(Self::integer("$$ffi.Int8")),
            Type::WaitResult => Ok(Self::integer("$$ffi.Int32")),
            Type::CallbackHandle(_) => Ok(Self::same("_$$BoltCallbackHandle")),
            Type::Named(name) | Type::DirectRecord(name) => {
                Ok(Self::same(format!("_$${}", name.as_str())))
            }
            Type::CStyleEnum { repr, .. } => Self::from_c(repr),
            Type::ConstPointer(inner) | Type::MutPointer(inner) => {
                Self::from_c(inner).map(Self::pointer)
            }
            Type::FunctionPointer { returns, params } => {
                let returns = Self::from_c(returns)?;
                let params = params
                    .iter()
                    .map(Self::from_c)
                    .collect::<Result<Vec<_>>>()?;
                let signature = format!(
                    "{} Function({})",
                    returns.native,
                    params
                        .iter()
                        .map(|parameter| parameter.native.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                Ok(Self::same(format!(
                    "$$ffi.Pointer<$$ffi.NativeFunction<{signature}>>"
                )))
            }
        }
    }

    pub fn primitive(primitive: Primitive) -> Result<Self> {
        Self::from_c(&Type::primitive(primitive)?)
    }

    pub fn native(&self) -> &str {
        &self.native
    }

    pub fn dart(&self) -> &str {
        &self.dart
    }

    fn new(native: impl Into<String>, dart: impl Into<String>) -> Self {
        Self {
            native: native.into(),
            dart: dart.into(),
        }
    }

    fn same(spelling: impl Into<String>) -> Self {
        let spelling = spelling.into();
        Self::new(spelling.clone(), spelling)
    }

    fn integer(native: impl Into<String>) -> Self {
        Self::new(native, "int")
    }

    fn floating(native: impl Into<String>) -> Self {
        Self::new(native, "double")
    }

    fn pointer(inner: Self) -> Self {
        Self::same(format!("$$ffi.Pointer<{}>", inner.native))
    }
}

pub fn declaration(function: &Function) -> Result<String> {
    let parameters = function
        .params()
        .iter()
        .map(|parameter| {
            NativeType::from_c(parameter.ty()).and_then(|ty| {
                parameter_name(parameter.name()).and_then(|name| {
                    Identifier::parse(name).map(|name| NativeParameter { name, ty })
                })
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let declaration = NativeFunction {
        returns: NativeType::from_c(function.returns())?,
        name: Identifier::parse(function.name())?,
        leaf: function.source_declaration().is_none()
            && parameters
                .iter()
                .all(|parameter| !parameter.ty.native().contains("NativeFunction")),
        parameters,
    };
    Ok(NativeFunctionTemplate {
        function: &declaration,
    }
    .render()
    .expect("rendering an in-memory Dart native-function template cannot fail"))
}

impl NativeFunction {
    fn returns(&self) -> &NativeType {
        &self.returns
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn parameters(&self) -> &[NativeParameter] {
        &self.parameters
    }

    fn leaf(&self) -> bool {
        self.leaf
    }
}

impl NativeParameter {
    fn name(&self) -> &Identifier {
        &self.name
    }

    fn ty(&self) -> &NativeType {
        &self.ty
    }
}

pub fn parameter_name(name: &str) -> Result<String> {
    Identifier::normalize(name).map(|name| name.to_string())
}

pub fn bridge_function<'bridge>(
    symbol: &boltffi_binding::NativeSymbol,
    functions: &'bridge [Function],
) -> Result<&'bridge Function> {
    functions
        .iter()
        .find(|function| function.source_symbol() == Some(symbol.id()))
        .ok_or(Error::BrokenBridgeContract {
            bridge: "c",
            invariant: "Dart callable symbol is missing from the C bridge",
        })
}

pub fn pointer_read(ty: &Type, pointer: &str) -> Result<String> {
    Ok(match ty {
        Type::Status
        | Type::Buffer
        | Type::String
        | Type::Span
        | Type::CallbackHandle(_)
        | Type::Named(_)
        | Type::DirectRecord(_) => format!("{pointer}.ref"),
        Type::CStyleEnum { repr, .. } => return pointer_read(repr, pointer),
        Type::Void => {
            return Err(Error::UnexpectedBindingShape {
                layer: "dart native pointer",
                shape: "void out-pointer",
            });
        }
        _ => format!("{pointer}.value"),
    })
}
