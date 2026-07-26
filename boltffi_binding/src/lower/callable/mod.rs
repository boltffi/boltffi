//! Lowers AST callables (methods, initializers, free functions) into
//! the [`CallableDecl<S, K>`] family.
//!
//! Each axis of the IR's call shape (receiver mode, parameter
//! crossings, return crossing, error transport, execution kind) is
//! decided here from the source [`MethodDef`] (and friends) and the
//! surrounding [`CallableOwner`] context. The resulting
//! [`CallableDecl`] carries every decision a renderer needs without
//! re-running the dispatch.
//!
//! Scope splits along which side implements the body:
//!
//! - [`lower_exported_method`] and [`lower_function`] produce
//!   [`ExportedCallable<S>`] (`K = RustBody`): Rust implements, foreign
//!   calls in. They take a [`SymbolAllocator`] and the start callable's
//!   symbol name because async exported callables mint the lifecycle
//!   symbols on [`Surface::AsyncProtocol`] from that prefix.
//! - [`lower_imported_method`] produces an [`ImportedCallable<S>`]
//!   used by callback-trait dispatch (`K = ForeignBody`): foreign
//!   implements, Rust calls out. Imported async callables carry the
//!   surface protocol used by callback-trait dispatch.
//! - [`lower_closure_param_into_rust`] produces a closure parameter
//!   whose invoke contract is an [`ImportedCallable<S>`]. Closure
//!   signatures have no execution axis in the AST, so the invoke is
//!   always synchronous.
//!
//! What this module supports today:
//!
//! - synchronous and async exported callables, the latter through the
//!   surface's [`Surface::AsyncProtocol`] value built by
//!   [`super::async_protocol`];
//! - by-value, by-ref, and by-mut-ref receivers;
//! - callback-trait params and returns from `impl Trait`,
//!   `Box<dyn Trait>`, `Arc<dyn Trait>`, and their optional forms,
//!   routed as nullable or required callback handles;
//! - `Result<(), E>` returns, which produce a void plan plus an
//!   encoded error channel;
//! - parameter and return types that lower through the existing
//!   [`super::types`] and [`super::codecs`] passes.
//!
//! What it rejects with a precise error (each is a follow-up gap):
//!
//! - the unit type `()` outside the `Result<(), E>` success channel
//!   ([`UnsupportedType::UnitInValuePosition`]);
//! - `Self` referenced from a callback-trait method signature
//!   ([`UnsupportedType::SelfInCallbackTrait`]);
//! - parameters whose type references a declaration family the pass
//!   has not yet lowered. Those are caught upstream by
//!   [`super::reject_unsupported`] so they cannot reach here.
//!
//! [`ExportedCallable<S>`]: crate::ExportedCallable
//! [`ImportedCallable<S>`]: crate::ImportedCallable
//! [`Surface::AsyncProtocol`]: crate::Surface::AsyncProtocol

mod params;
mod returns;

use boltffi_ast::{
    BaseTrait, CanonicalName as SourceName, ClassId, ExecutionKind, FnSig, FnTrait, FunctionDef,
    MethodDef, ParameterDef, Receiver, ReturnDef, TraitBounds, TraitId, TypeExpr,
};

use crate::{
    ClosureForm, ClosureParameter, ClosureRegistration, ClosureReturn, ClosureSignature,
    DirectVectorElementType, Direction, ExecutionDecl, ExportedCallable, ForeignBody,
    HandlePresence, ImportedCallable, IntoRust, OutOfRust, Primitive, Receive, RustBody,
};

use super::{
    LowerError, codecs, error::UnsupportedType, ids::DeclarationIds, index::Index, records,
    surface::SurfaceLower, symbol::SymbolAllocator,
};

/// Names the declaration that owns a callable.
///
/// Used to resolve `Self` inside parameter and return types and to
/// drive the symbol-naming convention. Carries a borrow into the
/// source AST so the lowering pass does not duplicate identity.
#[derive(Clone, Copy)]
pub enum CallableOwner<'src> {
    /// Owned by a record.
    Record(&'src boltffi_ast::RecordDef),
    /// Owned by an enum.
    Enum(&'src boltffi_ast::EnumDef),
    /// Owned by a class.
    Class(&'src boltffi_ast::ClassDef),
    /// Owned by a trait.
    Trait(&'src boltffi_ast::TraitDef),
    /// A top-level free function. Free functions have no `Self` and no
    /// owning type, so type-expression substitution rejects any `Self`
    /// reference encountered in this position.
    Function,
}

impl<'src> CallableOwner<'src> {
    fn self_type_expr(self) -> Result<boltffi_ast::TypeExpr, LowerError> {
        match self {
            Self::Record(record) => Ok(boltffi_ast::TypeExpr::record(
                record.id.clone(),
                boltffi_ast::Path::single("Self"),
            )),
            Self::Enum(enumeration) => Ok(boltffi_ast::TypeExpr::enumeration(
                enumeration.id.clone(),
                boltffi_ast::Path::single("Self"),
            )),
            Self::Class(class) => Ok(boltffi_ast::TypeExpr::class(
                class.id.clone(),
                boltffi_ast::Path::single("Self"),
            )),
            Self::Trait(_) => Err(LowerError::unsupported_type(
                UnsupportedType::SelfInCallbackTrait,
            )),
            Self::Function => Err(LowerError::unsupported_type(UnsupportedType::SelfType)),
        }
    }

    pub fn owns_type_expr(self, type_expr: &boltffi_ast::TypeExpr) -> bool {
        match (self, type_expr) {
            (Self::Trait(_) | Self::Function, boltffi_ast::TypeExpr::SelfType) => false,
            (_, boltffi_ast::TypeExpr::SelfType) => true,
            (Self::Record(record), boltffi_ast::TypeExpr::Record { id, .. }) => id == &record.id,
            (Self::Enum(enumeration), boltffi_ast::TypeExpr::Enum { id, .. }) => {
                id == &enumeration.id
            }
            (Self::Class(class), boltffi_ast::TypeExpr::Class { id, .. }) => id == &class.id,
            (
                Self::Trait(source_trait),
                boltffi_ast::TypeExpr::ImplTrait(bounds) | boltffi_ast::TypeExpr::Dyn(bounds),
            ) => match &bounds.base {
                boltffi_ast::BaseTrait::Named { id, .. } => id == &source_trait.id,
                boltffi_ast::BaseTrait::Function(_) => false,
            },
            _ => false,
        }
    }
}

enum ValueSpecialization {
    ScalarOption(Primitive),
    DirectVector(DirectVectorElementType),
}

impl ValueSpecialization {
    fn from_type_expr<S: SurfaceLower>(
        index: &Index,
        ids: &DeclarationIds,
        type_expr: &TypeExpr,
    ) -> Result<Option<Self>, LowerError> {
        Self::from_parameter::<S>(index, ids, type_expr, Receive::ByValue)
    }

    fn from_parameter<S: SurfaceLower>(
        index: &Index,
        ids: &DeclarationIds,
        type_expr: &TypeExpr,
        receive: Receive,
    ) -> Result<Option<Self>, LowerError> {
        match (type_expr, receive) {
            (TypeExpr::Option(inner), Receive::ByValue) => Ok(Self::primitive(inner)
                .and_then(S::scalar_option)
                .map(Self::ScalarOption)),
            (TypeExpr::Vec(inner), Receive::ByValue) => {
                Self::direct_vector_element(index, ids, inner)
                    .map(|element| element.map(Self::DirectVector))
            }
            (TypeExpr::Slice(inner), Receive::ByRef | Receive::ByMutRef) => {
                Ok(Self::primitive(inner)
                    .and_then(DirectVectorElementType::primitive)
                    .map(Self::DirectVector))
            }
            _ => Ok(None),
        }
    }

    fn primitive(type_expr: &TypeExpr) -> Option<Primitive> {
        if let TypeExpr::Primitive(primitive) = type_expr {
            Some(Primitive::from(*primitive))
        } else {
            None
        }
    }

    fn direct_vector_element(
        index: &Index,
        ids: &DeclarationIds,
        type_expr: &TypeExpr,
    ) -> Result<Option<DirectVectorElementType>, LowerError> {
        match type_expr {
            TypeExpr::Primitive(primitive) => Ok(DirectVectorElementType::primitive(
                Primitive::from(*primitive),
            )),
            TypeExpr::Record { id, .. } if index.record(id).is_some_and(records::is_direct) => {
                Ok(Some(DirectVectorElementType::record(ids.record(id)?)))
            }
            _ => Ok(None),
        }
    }
}

/// Lowers a Rust-implemented [`MethodDef`] into an
/// [`ExportedCallable<S>`].
///
/// `start_symbol_name` is the symbol foreign code links against to
/// invoke this callable. For sync methods it is the only symbol the
/// callable references; for async methods it is the prefix used to
/// mint the lifecycle symbols on [`Surface::AsyncProtocol`]. The
/// allocator hands out fresh ids for each lifecycle symbol.
///
/// The owner context resolves `Self` inside parameter and return type
/// expressions. The receiver follows the source.
pub fn lower_exported_method<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner,
    method: &MethodDef,
    start_symbol_name: &str,
) -> Result<ExportedCallable<S>, LowerError> {
    let receiver = lower_receiver(method.receiver);
    let parameters = params::lower::<S, IntoRust>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Surface,
        &method.parameters,
    )?;
    let (returns, error) = returns::lower::<S, _>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Surface,
        &method.returns,
    )?;
    let execution = lower_execution::<S>(allocator, method.execution, start_symbol_name)?;

    Ok(ExportedCallable::<S>::new(
        receiver, parameters, returns, error, execution,
    )?)
}

/// Lowers a foreign-implemented callback trait [`MethodDef`] into an
/// [`ImportedCallable<S>`].
///
/// Callback methods cross in the opposite direction from exported
/// methods: Rust pushes arguments out and reads the return back in.
/// Their dispatch target is not a [`NativeSymbol`](crate::NativeSymbol)
/// but a per-surface slot ([`crate::VTableSlot`] on native, an
pub fn lower_imported_method<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner,
    method: &MethodDef,
    execution: ExecutionDecl<S>,
) -> Result<ImportedCallable<S>, LowerError> {
    let receiver = lower_receiver(method.receiver);
    let parameters = params::lower::<S, OutOfRust>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Framed,
        &method.parameters,
    )?;
    let (returns, error) = returns::lower::<S, IntoRust>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Framed,
        &method.returns,
    )?;

    Ok(ImportedCallable::<S>::new(
        receiver, parameters, returns, error, execution,
    )?)
}

pub fn lower_local_callback_method<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner,
    method: &MethodDef,
) -> Result<ExportedCallable<S>, LowerError> {
    let parameters = params::lower::<S, IntoRust>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Framed,
        &method.parameters,
    )?;
    let (returns, error) = returns::lower::<S, OutOfRust>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Framed,
        &method.returns,
    )?;

    Ok(ExportedCallable::<S>::new(
        None,
        parameters,
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?)
}

/// Lowers an inline closure crossing into Rust as a parameter.
///
/// The closure was created on the foreign side, so its body lives
/// there. Rust holds the handle and invokes it. The invoke contract is
/// an [`ImportedCallable<S>`] (`K = ForeignBody`): args flow
/// [`OutOfRust`] at invocation, returns and error flow back as
/// [`IntoRust`]. The registration uses [`Receive::ByValue`] and the
/// surface's closure-registration shape.
///
/// Closure parameters have no source names, so the lowering pass
/// synthesises `arg0`, `arg1`, ... and reuses the regular parameter
/// machinery against them. Closure signatures have no execution axis in
/// the AST, so the invoke is always synchronous. `Self` references
/// reach the function-scoped substitution path and are rejected there.
fn lower_closure_param_into_rust<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: ClosureSource,
) -> Result<ClosureParameter<S, IntoRust>, LowerError> {
    let parts = lower_closure_into_rust_parts(index, ids, allocator, closure)?;
    Ok(ClosureParameter::new(
        parts.form,
        parts.signature,
        parts.presence,
        parts.registration,
        parts.invoke,
    ))
}

fn lower_closure_return_into_rust<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: ClosureSource,
) -> Result<ClosureReturn<S, IntoRust>, LowerError> {
    let parts = lower_closure_into_rust_parts(index, ids, allocator, closure)?;
    Ok(ClosureReturn::new(
        parts.form,
        parts.signature,
        parts.presence,
        parts.registration,
        parts.invoke,
    ))
}

struct ClosureIntoRustParts<S: crate::Surface> {
    form: ClosureForm,
    signature: ClosureSignature,
    presence: HandlePresence,
    registration: ClosureRegistration<S, IntoRust>,
    invoke: ImportedCallable<S>,
}

fn lower_closure_into_rust_parts<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: ClosureSource,
) -> Result<ClosureIntoRustParts<S>, LowerError> {
    let (parameters, returns, error) =
        lower_closure_invoke_parts::<S, ForeignBody>(index, ids, allocator, closure.signature)?;
    let invoke = ImportedCallable::<S>::new(
        None,
        parameters,
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?;
    let registration = ClosureRegistration::<S, IntoRust>::new(
        S::incoming_closure_registration(closure.signature)?,
        Receive::ByValue,
    );
    Ok(ClosureIntoRustParts {
        form: closure.form,
        signature: ClosureSignature::from_fn_signature(closure.signature),
        presence: closure.presence,
        registration,
        invoke,
    })
}

/// Lowers an inline closure crossing out of Rust as a callback
/// parameter.
///
/// The closure was created on the Rust side and crosses out so foreign
/// code can invoke it from a foreign-body callable (callback method).
/// The body lives on the Rust side, so the invoke contract is an
/// [`ExportedCallable<S>`] (`K = RustBody`): args flow [`IntoRust`] at
/// invocation, returns and error flow back as [`OutOfRust`]. The
/// registration carries the surface's closure-registration shape with a
/// `()` receive slot.
fn lower_closure_param_out_of_rust<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: ClosureSource,
) -> Result<ClosureParameter<S, OutOfRust>, LowerError> {
    let parts = lower_closure_out_of_rust_parts(index, ids, allocator, closure)?;
    Ok(ClosureParameter::new(
        parts.form,
        parts.signature,
        parts.presence,
        parts.registration,
        parts.invoke,
    ))
}

fn lower_closure_return_out_of_rust<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: ClosureSource,
) -> Result<ClosureReturn<S, OutOfRust>, LowerError> {
    let parts = lower_closure_out_of_rust_parts(index, ids, allocator, closure)?;
    Ok(ClosureReturn::new(
        parts.form,
        parts.signature,
        parts.presence,
        parts.registration,
        parts.invoke,
    ))
}

struct ClosureOutOfRustParts<S: crate::Surface> {
    form: ClosureForm,
    signature: ClosureSignature,
    presence: HandlePresence,
    registration: ClosureRegistration<S, OutOfRust>,
    invoke: ExportedCallable<S>,
}

fn lower_closure_out_of_rust_parts<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: ClosureSource,
) -> Result<ClosureOutOfRustParts<S>, LowerError> {
    let (parameters, returns, error) =
        lower_closure_invoke_parts::<S, RustBody>(index, ids, allocator, closure.signature)?;
    let invoke = ExportedCallable::<S>::new(
        None,
        parameters,
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?;
    let shape = S::outgoing_closure_registration(allocator, closure.signature)?;
    #[allow(clippy::let_unit_value)]
    let receive = <OutOfRust as Direction>::receive_from(Receive::ByValue);
    let registration = ClosureRegistration::<S, OutOfRust>::new(shape, receive);
    Ok(ClosureOutOfRustParts {
        form: closure.form,
        signature: ClosureSignature::from_fn_signature(closure.signature),
        presence: closure.presence,
        registration,
        invoke,
    })
}

type ClosureInvokeParts<S, K> = (
    Vec<crate::ParamDecl<S, <K as crate::CallableScope>::ParamDirection>>,
    crate::ReturnDecl<S, <K as crate::CallableScope>::ReturnDirection>,
    crate::ErrorDecl<S, <K as crate::CallableScope>::ReturnDirection>,
);

fn lower_closure_invoke_parts<S: SurfaceLower, K: crate::CallableScope>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &FnSig,
) -> Result<ClosureInvokeParts<S, K>, LowerError>
where
    K::ParamDirection: params::LowerClosure<S>,
    K::ReturnDirection: params::LowerClosure<S>,
{
    let owner = CallableOwner::Function;
    let parameters = closure
        .parameters
        .iter()
        .enumerate()
        .map(|(parameter_index, type_expr)| {
            ParameterDef::value(
                SourceName::single(format!("arg{parameter_index}")),
                type_expr.clone(),
            )
        })
        .collect::<Vec<_>>();
    let lowered_params = params::lower::<S, K::ParamDirection>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Framed,
        &parameters,
    )?;
    let (returns, error) = returns::lower::<S, K::ReturnDirection>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Framed,
        &closure.returns,
    )?;
    Ok((lowered_params, returns, error))
}

/// Lowers one [`FunctionDef`] into an [`ExportedCallable<S>`].
///
/// Free functions have no receiver and no `Self`; the owner context is
/// [`CallableOwner::Function`], which rejects any `Self` reference
/// found while walking parameter and return types. Async free
/// functions lower through the same lifecycle protocol as async
/// methods; `start_symbol_name` names the start symbol foreign code
/// links to invoke the operation, and the lifecycle symbols are minted
/// with that name as prefix.
pub fn lower_function<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    function: &FunctionDef,
    start_symbol_name: &str,
) -> Result<ExportedCallable<S>, LowerError> {
    let owner = CallableOwner::Function;
    let parameters = params::lower::<S, IntoRust>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Surface,
        &function.parameters,
    )?;
    let (returns, error) = returns::lower::<S, _>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Surface,
        &function.returns,
    )?;
    let execution = lower_execution::<S>(allocator, function.execution, start_symbol_name)?;

    Ok(ExportedCallable::<S>::new(
        None, parameters, returns, error, execution,
    )?)
}

/// Lowers a zero-argument getter for a constant whose value cannot be
/// delivered as an inline literal.
///
/// The accessor is a synchronous [`ExportedCallable<S>`] (Rust
/// implements, foreign calls in) with no receiver, no parameters, and no
/// error channel, returning the constant's declared type. Foreign code
/// reads the value by calling it once. The owner context is
/// [`CallableOwner::Function`], so any `Self` in the constant type is
/// rejected; top-level constants do not carry `Self`.
pub fn lower_constant_accessor<S: SurfaceLower>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    type_expr: &boltffi_ast::TypeExpr,
) -> Result<ExportedCallable<S>, LowerError> {
    let owner = CallableOwner::Function;
    let return_def = boltffi_ast::ReturnDef::value(type_expr.clone());
    let (returns, error) = returns::lower::<S, _>(
        index,
        ids,
        allocator,
        owner,
        codecs::RootEncoding::Surface,
        &return_def,
    )?;

    Ok(ExportedCallable::<S>::new(
        None,
        Vec::new(),
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?)
}

fn lower_execution<S: SurfaceLower>(
    allocator: &mut SymbolAllocator,
    execution: ExecutionKind,
    start_symbol_name: &str,
) -> Result<ExecutionDecl<S>, LowerError> {
    match execution {
        ExecutionKind::Sync => Ok(ExecutionDecl::synchronous()),
        ExecutionKind::Async => {
            let protocol = S::build_protocol(allocator, start_symbol_name)?;
            Ok(ExecutionDecl::asynchronous(protocol))
        }
    }
}

fn lower_receiver(receiver: Receiver) -> Option<Receive> {
    match receiver {
        Receiver::None => None,
        Receiver::Shared => Some(Receive::ByRef),
        Receiver::Mutable => Some(Receive::ByMutRef),
        Receiver::Owned => Some(Receive::ByValue),
    }
}

#[derive(Clone, Copy)]
struct ClosureSource<'source> {
    form: ClosureForm,
    signature: &'source FnSig,
    presence: HandlePresence,
}

impl<'source> ClosureSource<'source> {
    fn from_type_expr(type_expr: &'source TypeExpr) -> Option<Self> {
        Self::required(type_expr).or_else(|| match type_expr {
            TypeExpr::Option(inner) => Self::required(inner).map(Self::nullable),
            _ => None,
        })
    }

    fn required(type_expr: &'source TypeExpr) -> Option<Self> {
        match type_expr {
            TypeExpr::FnPtr(signature) => Some(Self {
                form: ClosureForm::FunctionPointer,
                signature,
                presence: HandlePresence::Required,
            }),
            TypeExpr::ImplTrait(bounds) => match &bounds.base {
                BaseTrait::Function(function_trait) => Some(Self {
                    form: ClosureForm::from(function_trait.kind),
                    signature: &function_trait.signature,
                    presence: HandlePresence::Required,
                }),
                BaseTrait::Named { .. } => None,
            },
            TypeExpr::Boxed(inner) => match inner.as_ref() {
                TypeExpr::Dyn(bounds) => match &bounds.base {
                    BaseTrait::Function(function_trait) => Some(Self {
                        form: ClosureForm::from(function_trait.kind),
                        signature: &function_trait.signature,
                        presence: HandlePresence::Required,
                    }),
                    BaseTrait::Named { .. } => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn nullable(mut self) -> Self {
        self.presence = HandlePresence::Nullable;
        self
    }
}

#[derive(Clone, Copy)]
struct ClassHandleSource<'source> {
    id: &'source ClassId,
    presence: HandlePresence,
}

impl<'source> ClassHandleSource<'source> {
    fn from_type_expr(type_expr: &'source TypeExpr) -> Option<Self> {
        Self::required(type_expr).or_else(|| match type_expr {
            TypeExpr::Option(inner) => Self::required(inner).map(Self::nullable),
            _ => None,
        })
    }

    fn required(type_expr: &'source TypeExpr) -> Option<Self> {
        match type_expr {
            TypeExpr::Class { id, .. } => Some(Self {
                id,
                presence: HandlePresence::Required,
            }),
            _ => None,
        }
    }

    fn nullable(mut self) -> Self {
        self.presence = HandlePresence::Nullable;
        self
    }
}

#[derive(Clone, Copy)]
struct CallbackHandleSource<'source> {
    id: &'source TraitId,
    presence: HandlePresence,
}

impl<'source> CallbackHandleSource<'source> {
    fn from_type_expr(type_expr: &'source TypeExpr) -> Option<Self> {
        Self::required(type_expr).or_else(|| match type_expr {
            TypeExpr::Option(inner) => Self::required(inner).map(Self::nullable),
            _ => None,
        })
    }

    fn bare_dyn(type_expr: &'source TypeExpr) -> Option<Self> {
        match type_expr {
            TypeExpr::Dyn(bounds) => match &bounds.base {
                BaseTrait::Named { id, .. } => Some(Self {
                    id,
                    presence: HandlePresence::Required,
                }),
                BaseTrait::Function(_) => None,
            },
            _ => None,
        }
    }

    fn required(type_expr: &'source TypeExpr) -> Option<Self> {
        match type_expr {
            TypeExpr::ImplTrait(bounds) => match &bounds.base {
                BaseTrait::Named { id, .. } => Some(Self {
                    id,
                    presence: HandlePresence::Required,
                }),
                BaseTrait::Function(_) => None,
            },
            TypeExpr::Boxed(inner) | TypeExpr::Arc(inner) => match inner.as_ref() {
                TypeExpr::Dyn(bounds) => match &bounds.base {
                    BaseTrait::Named { id, .. } => Some(Self {
                        id,
                        presence: HandlePresence::Required,
                    }),
                    BaseTrait::Function(_) => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn nullable(mut self) -> Self {
        self.presence = HandlePresence::Nullable;
        self
    }
}

/// Substitutes occurrences of [`TypeExpr::SelfType`] with the owner's
/// concrete type expression.
///
/// Walks the expression once. Other `Self`-shaped sub-expressions
/// (`Vec<Self>`, `Option<Self>`, tuple elements, map keys/values,
/// closure parameters and returns, optional/sequence inner) all
/// recurse so a method like `fn neighbours(&self) -> Vec<Self>`
/// resolves correctly.
pub fn substitute_self_type(
    owner: CallableOwner,
    type_expr: &boltffi_ast::TypeExpr,
) -> Result<boltffi_ast::TypeExpr, LowerError> {
    use boltffi_ast::TypeExpr;
    Ok(match type_expr {
        TypeExpr::SelfType => owner.self_type_expr()?,
        TypeExpr::Vec(inner) => TypeExpr::Vec(Box::new(substitute_self_type(owner, inner)?)),
        TypeExpr::Slice(inner) => TypeExpr::Slice(Box::new(substitute_self_type(owner, inner)?)),
        TypeExpr::Option(inner) => TypeExpr::Option(Box::new(substitute_self_type(owner, inner)?)),
        TypeExpr::Boxed(inner) => TypeExpr::Boxed(Box::new(substitute_self_type(owner, inner)?)),
        TypeExpr::Arc(inner) => TypeExpr::Arc(Box::new(substitute_self_type(owner, inner)?)),
        TypeExpr::Tuple(elements) => TypeExpr::Tuple(
            elements
                .iter()
                .map(|element| substitute_self_type(owner, element))
                .collect::<Result<Vec<_>, LowerError>>()?,
        ),
        TypeExpr::Map { kind, key, value } => TypeExpr::Map {
            kind: *kind,
            key: Box::new(substitute_self_type(owner, key)?),
            value: Box::new(substitute_self_type(owner, value)?),
        },
        TypeExpr::Result { ok, err } => TypeExpr::Result {
            ok: Box::new(substitute_self_type(owner, ok)?),
            err: Box::new(substitute_self_type(owner, err)?),
        },
        TypeExpr::FnPtr(signature) => {
            TypeExpr::FnPtr(Box::new(substitute_self_signature(owner, signature)?))
        }
        TypeExpr::Dyn(bound) => TypeExpr::Dyn(substitute_self_trait_bound(owner, bound)?),
        TypeExpr::ImplTrait(bound) => {
            TypeExpr::ImplTrait(substitute_self_trait_bound(owner, bound)?)
        }
        TypeExpr::Primitive(_)
        | TypeExpr::Unit
        | TypeExpr::String
        | TypeExpr::Str
        | TypeExpr::InternedString { .. }
        | TypeExpr::Builtin(_)
        | TypeExpr::Record { .. }
        | TypeExpr::Enum { .. }
        | TypeExpr::Class { .. }
        | TypeExpr::Custom { .. }
        | TypeExpr::Parameter(_) => type_expr.clone(),
    })
}

fn substitute_self_signature(owner: CallableOwner, signature: &FnSig) -> Result<FnSig, LowerError> {
    Ok(FnSig::new(
        signature
            .parameters
            .iter()
            .map(|parameter| substitute_self_type(owner, parameter))
            .collect::<Result<Vec<_>, LowerError>>()?,
        match &signature.returns {
            ReturnDef::Void => ReturnDef::Void,
            ReturnDef::Value(value) => ReturnDef::Value(substitute_self_type(owner, value)?),
        },
    ))
}

fn substitute_self_trait_bound(
    owner: CallableOwner,
    bounds: &TraitBounds,
) -> Result<TraitBounds, LowerError> {
    let base = match &bounds.base {
        BaseTrait::Named { .. } => bounds.base.clone(),
        BaseTrait::Function(function_trait) => BaseTrait::Function(Box::new(FnTrait::new(
            function_trait.kind,
            substitute_self_signature(owner, &function_trait.signature)?,
        ))),
    };
    Ok(TraitBounds::new(base, bounds.bounds.clone()))
}
