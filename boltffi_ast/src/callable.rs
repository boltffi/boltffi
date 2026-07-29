use serde::{Deserialize, Serialize};

use crate::TypeExpr;
use crate::{
    DefaultValue, DeprecationInfo, DocComment, FunctionId, MethodId, Source, SourceName,
    SourceSpan, UserAttr,
};

/// The place where a callable appears in the Rust API.
///
/// Free functions, methods with receivers, and associated functions without
/// receivers are all callable, but they live in different parts of Rust source.
/// This enum keeps that source placement close to the signature.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum CallableForm {
    /// A free function.
    Function,
    /// A method with a receiver.
    Method,
    /// An associated function without a receiver.
    AssociatedFunction,
}

/// Whether a callable was written as synchronous or asynchronous Rust.
///
/// The value comes directly from the Rust signature. A function written with
/// `async fn` is `Async`; every other callable is `Sync`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ExecutionKind {
    /// A callable without `async`.
    Sync,
    /// A callable written with `async`.
    Async,
}

/// A parameter in a function, method, or callback method.
///
/// The parameter records both the type and the way it was accepted by Rust,
/// such as by value, by shared reference, or by mutable reference.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ParameterDef {
    /// Source parameter name.
    pub name: SourceName,
    /// Rust source type after known FFI names have been identified.
    pub type_expr: TypeExpr,
    /// How the parameter was accepted by the Rust callable.
    pub passing: ParameterPassing,
    /// Documentation attached to the parameter when the source provides it.
    pub doc: Option<DocComment>,
    /// Default value written for bindings that expose default arguments.
    pub default: Option<DefaultValue>,
    /// User attributes preserved from the source parameter.
    pub user_attrs: Vec<UserAttr>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
}

impl ParameterDef {
    /// Builds a value parameter with no documentation, attributes, or default.
    ///
    /// The `name` parameter is the canonical parameter name. The `rust_type`
    /// parameter is the scanned Rust source type.
    ///
    /// Returns a parameter that was passed by value in Rust source.
    pub fn value(name: impl Into<SourceName>, type_expr: TypeExpr) -> Self {
        Self {
            name: name.into(),
            type_expr,
            passing: ParameterPassing::Value,
            doc: None,
            default: None,
            user_attrs: Vec::new(),
            source: Source::exported(),
        }
    }
}

/// The Rust passing form used by a parameter.
///
/// Records only the reference flavor (`T`, `&T`, `&mut T`). The trait use
/// form (`impl Trait`, `Box<dyn Trait>`, `Arc<dyn Trait>`) is a fact of the
/// type, not of the parameter slot, so it lives on
/// the type expression instead.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ParameterPassing {
    /// A value parameter such as `value: T`.
    Value,
    /// A shared reference parameter such as `value: &T`.
    Ref,
    /// A mutable reference parameter such as `value: &mut T`.
    RefMut,
}

/// The outermost return type of a callable.
///
/// Every non-void return is represented as `Value(TypeExpr)`, including
/// `Result<T, E>`. Fallibility is part of the type expression so the lowering
/// layer can decide how the success value and error channel cross the boundary
/// from one source shape.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ReturnDef {
    /// The callable returns `()`.
    Void,
    /// The callable returns one value.
    ///
    /// This includes tuples, options, and results. For example,
    /// `fn pair() -> (u32, String)` is `Value(TypeExpr::Tuple(_))`, and
    /// `fn try_open() -> Result<File, Error>` is `Value(TypeExpr::Result { .. })`.
    Value(TypeExpr),
}

impl ReturnDef {
    /// Builds a value return.
    pub fn value(type_expr: TypeExpr) -> Self {
        Self::Value(type_expr)
    }

    /// Builds a return definition from an optional value type.
    ///
    /// The `type_expr` parameter is `None` for `()` and `Some` for a returned value.
    ///
    /// Returns `Void` for no value and `Value` for a present Rust type.
    pub fn from_value(type_expr: Option<TypeExpr>) -> Self {
        match type_expr {
            Some(type_expr) => Self::Value(type_expr),
            None => Self::Void,
        }
    }
}

/// The receiver written on a Rust method.
///
/// This is the part of the method signature before the ordinary parameters:
/// no receiver, `&self`, `&mut self`, or `self`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Receiver {
    /// A method with no receiver.
    None,
    /// A method taking `&self`.
    Shared,
    /// A method taking `&mut self`.
    Mutable,
    /// A method taking `self`.
    Owned,
}

impl Receiver {
    /// Returns the callable form implied by the receiver.
    ///
    /// The return value is the callable form implied by the receiver alone.
    pub const fn callable_form(self) -> CallableForm {
        match self {
            Self::None => CallableForm::AssociatedFunction,
            Self::Shared | Self::Mutable | Self::Owned => CallableForm::Method,
        }
    }
}

/// A free function exported from the source contract.
///
/// Functions are top-level callable declarations. Methods attached to records,
/// enums, classes, and callbacks use their own node types so ownership remains
/// explicit.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FunctionDef {
    /// Stable function identity derived from its canonical source path.
    pub id: FunctionId,
    /// Source function name.
    pub name: SourceName,
    /// Source callable form. For free functions this is always `Function`.
    pub form: CallableForm,
    /// Whether the Rust source used `async`.
    pub execution: ExecutionKind,
    /// Parameters written by the Rust function.
    pub parameters: Vec<ParameterDef>,
    /// Return type written by the Rust function.
    pub returns: ReturnDef,
    /// Documentation attached to the function.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the function.
    pub deprecated: Option<DeprecationInfo>,
    /// User attributes preserved from the function.
    pub user_attrs: Vec<UserAttr>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl FunctionDef {
    /// Builds a synchronous free function with no parameters and no return value.
    ///
    /// The `id` parameter is the stable function ID. The `name` parameter is the
    /// canonical source name.
    ///
    /// Returns a function definition ready for the scanner to fill with source
    /// details.
    pub fn new(id: crate::FunctionId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            form: CallableForm::Function,
            execution: ExecutionKind::Sync,
            parameters: Vec::new(),
            returns: ReturnDef::Void,
            doc: None,
            deprecated: None,
            user_attrs: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }
}

/// A method attached to a record, enum, class, or callback trait.
///
/// The owning declaration contains the method, so the method itself only needs
/// its local identity, receiver, signature, attributes, and documentation.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct MethodDef {
    /// Stable method identity within the owning declaration.
    pub id: MethodId,
    /// Source method name.
    pub name: SourceName,
    /// Receiver written on the Rust method.
    pub receiver: Receiver,
    /// Whether the Rust source used `async`.
    pub execution: ExecutionKind,
    /// Parameters after the receiver.
    pub parameters: Vec<ParameterDef>,
    /// Return type written by the Rust method.
    pub returns: ReturnDef,
    /// Documentation attached to the method.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the method.
    pub deprecated: Option<DeprecationInfo>,
    /// User attributes preserved from the method.
    pub user_attrs: Vec<UserAttr>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl MethodDef {
    /// Builds a synchronous method with no parameters and no return value.
    ///
    /// The `id` parameter is stable within the owning declaration. The `name`
    /// parameter is the canonical source method name. The `receiver` parameter
    /// records how `self` was written.
    ///
    /// Returns a method definition ready for scan-time details.
    pub fn new(id: MethodId, name: impl Into<SourceName>, receiver: Receiver) -> Self {
        Self {
            id,
            name: name.into(),
            receiver,
            execution: ExecutionKind::Sync,
            parameters: Vec::new(),
            returns: ReturnDef::Void,
            doc: None,
            deprecated: None,
            user_attrs: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }
}
