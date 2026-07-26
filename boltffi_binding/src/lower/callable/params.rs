use boltffi_ast::{ParameterDef, ParameterPassing, TypeExpr};

use crate::{
    CanonicalName, ClosureParameter, ClosureReturn, DirectValueType, Direction, ElementMeta,
    HandleTarget, IntoRust, OutOfRust, ParamDecl, ParamDirection, ParamPlan, Primitive, Receive,
    ValueRef,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata,
    records, surface::SurfaceLower, symbol::SymbolAllocator, types,
};

use super::{
    CallableOwner, CallbackHandleSource, ClassHandleSource, ClosureSource, ValueSpecialization,
    substitute_self_type,
};

/// Lowers the parameter list of a callable in direction `D`.
///
/// The caller picks `D` from the enclosing scope's `ParamDirection`:
/// [`IntoRust`](crate::IntoRust) for parameters of a Rust-implemented
/// callable, [`OutOfRust`](crate::OutOfRust) for parameters of a
/// foreign-implemented callable.
///
/// Value parameters lower through [`ParamPlan`]. Closure parameters
/// dispatch through the [`LowerClosure`] trait so each direction
/// supplies the AST-to-IR construction for its closure shape; both
/// directions yield a closure payload whose invoke contract resolves
/// to [`ImportedCallable<S>`] for `IntoRust` and
/// [`ExportedCallable<S>`] for `OutOfRust` through
/// [`Direction::InvokeScope`](crate::Direction::InvokeScope).
///
/// [`ImportedCallable<S>`]: crate::ImportedCallable
/// [`ExportedCallable<S>`]: crate::ExportedCallable
pub fn lower<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner,
    root_encoding: codecs::RootEncoding,
    parameters: &[ParameterDef],
) -> Result<Vec<ParamDecl<S, D>>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    parameters
        .iter()
        .map(|parameter| lower_one::<S, D>(index, ids, allocator, owner, root_encoding, parameter))
        .collect()
}

fn lower_one<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner,
    root_encoding: codecs::RootEncoding,
    parameter: &ParameterDef,
) -> Result<ParamDecl<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    let type_expr = substitute_self_type(owner, &parameter.type_expr)?;
    let receive = receive_for_passing(parameter.passing);
    let canonical_name = CanonicalName::from(&parameter.name);
    let meta = metadata::element_meta(parameter.doc.as_ref(), None, parameter.default.as_ref())?;
    if let Some(closure) = ClosureSource::from_type_expr(&type_expr) {
        if !matches!(receive, Receive::ByValue) {
            return Err(LowerError::unsupported_type(
                UnsupportedType::BorrowedCallbackParameter,
            ));
        }
        return D::lower_closure_param(index, ids, allocator, canonical_name, meta, closure);
    }
    let value = ValueRef::named(canonical_name.clone());
    let plan = lower_plain_plan::<S, D>(index, ids, root_encoding, &type_expr, value, receive)?;
    Ok(ParamDecl::value(canonical_name, meta, plan))
}

fn lower_plain_plan<S: SurfaceLower, D: Direction>(
    index: &Index,
    ids: &DeclarationIds,
    root_encoding: codecs::RootEncoding,
    type_expr: &TypeExpr,
    value: ValueRef,
    receive: Receive,
) -> Result<ParamPlan<S, D>, LowerError> {
    match specialize_param::<S, D>(index, ids, type_expr, receive)? {
        Some(plan) => Ok(plan),
        None => lower_plan::<S, D>(index, ids, root_encoding, type_expr, value, receive),
    }
}

fn specialize_param<S: SurfaceLower, D: Direction>(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    receive: Receive,
) -> Result<Option<ParamPlan<S, D>>, LowerError> {
    let specialization = ValueSpecialization::from_parameter::<S>(index, ids, type_expr, receive)?;
    Ok(match specialization {
        Some(ValueSpecialization::ScalarOption(primitive)) => {
            Some(ParamPlan::ScalarOption { primitive })
        }
        Some(ValueSpecialization::DirectVector(element)) => Some(ParamPlan::DirectVec {
            element,
            receive: D::receive_from(receive),
        }),
        None => None,
    })
}

fn receive_for_passing(passing: ParameterPassing) -> Receive {
    match passing {
        ParameterPassing::Value => Receive::ByValue,
        ParameterPassing::Ref => Receive::ByRef,
        ParameterPassing::RefMut => Receive::ByMutRef,
    }
}

fn lower_plan<S: SurfaceLower, D: Direction>(
    index: &Index,
    ids: &DeclarationIds,
    root_encoding: codecs::RootEncoding,
    type_expr: &TypeExpr,
    value: ValueRef,
    receive: Receive,
) -> Result<ParamPlan<S, D>, LowerError> {
    if let Some(handle) = ClassHandleSource::from_type_expr(type_expr) {
        return Ok(ParamPlan::Handle {
            target: HandleTarget::Class(ids.class(handle.id)?),
            carrier: S::class_handle_carrier(),
            presence: handle.presence,
            receive: D::receive_from(receive),
        });
    }
    if let Some(handle) = CallbackHandleSource::from_type_expr(type_expr) {
        if !matches!(receive, Receive::ByValue) {
            return Err(LowerError::unsupported_type(
                UnsupportedType::BorrowedCallbackParameter,
            ));
        }
        return Ok(ParamPlan::Handle {
            target: HandleTarget::Callback(ids.callback(handle.id)?),
            carrier: S::callback_handle_carrier(),
            presence: handle.presence,
            receive: D::receive_from(receive),
        });
    }
    if CallbackHandleSource::bare_dyn(type_expr).is_some() && !matches!(receive, Receive::ByValue) {
        return Err(LowerError::unsupported_type(
            UnsupportedType::BorrowedCallbackParameter,
        ));
    }
    match type_expr {
        TypeExpr::Primitive(primitive) => Ok(ParamPlan::Direct {
            ty: DirectValueType::primitive(Primitive::from(*primitive)),
            receive: D::receive_from(receive),
        }),
        TypeExpr::Record { id, .. } if index.record(id).is_some_and(records::is_direct) => {
            Ok(ParamPlan::Direct {
                ty: DirectValueType::record(ids.record(id)?),
                receive: D::receive_from(receive),
            })
        }
        TypeExpr::Enum { id, .. } if index.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(ParamPlan::Direct {
                ty: DirectValueType::enumeration(ids.enumeration(id)?),
                receive: D::receive_from(receive),
            })
        }
        TypeExpr::String
        | TypeExpr::Str
        | TypeExpr::InternedString { .. }
        | TypeExpr::Builtin(_)
        | TypeExpr::Slice(_)
        | TypeExpr::Record { .. }
        | TypeExpr::Enum { .. }
        | TypeExpr::Vec(_)
        | TypeExpr::Option(_)
        | TypeExpr::Tuple(_)
        | TypeExpr::Result { .. }
        | TypeExpr::Map { .. }
        | TypeExpr::Custom { .. } => {
            let ty = types::lower(ids, type_expr)?;
            let codec_node = root_encoding.node::<S>(index, ids, type_expr, value.clone())?;
            Ok(ParamPlan::Encoded {
                ty,
                codec: D::make_codec(value, codec_node),
                shape: S::encoded_param_shape(),
                receive: D::receive_from(receive),
            })
        }
        _ if ClosureSource::from_type_expr(type_expr).is_some() => {
            unreachable!(
                "closure source type reached parameter value-plan lowering after the closure classifier should have built a closure parameter plan"
            )
        }
        TypeExpr::Unit
        | TypeExpr::SelfType
        | TypeExpr::Parameter(_)
        | TypeExpr::Class { .. }
        | TypeExpr::FnPtr(_)
        | TypeExpr::ImplTrait(_)
        | TypeExpr::Dyn(_)
        | TypeExpr::Boxed(_)
        | TypeExpr::Arc(_) => {
            Err(types::lower(ids, type_expr).expect_err(
                "parameter value-plan lowering reached a source type reserved for handle, closure, owner-substitution, or generic rejection before the direct/encoded fallback",
            ))
        }
    }
}

/// Lowers an inline closure crossing in one direction.
///
/// Implemented for [`IntoRust`] (closure travels foreign → Rust; the
/// invoke contract is an [`ImportedCallable<S>`](crate::ImportedCallable))
/// and for [`OutOfRust`] (closure travels Rust → foreign; the invoke
/// contract is an [`ExportedCallable<S>`](crate::ExportedCallable)).
/// The trait dispatches between [`super::lower_closure_param_into_rust`]
/// and [`super::lower_closure_param_out_of_rust`] without exposing a
/// direction-agnostic closure value to the surrounding walk.
///
/// Two wrapping helpers — [`Self::lower_closure_param`] for parameter
/// slots and [`Self::lower_closure_return`] for return slots — produce
/// the right position-shaped IR variant.
pub trait LowerClosure<S: SurfaceLower>: ParamDirection<S> + Sized
where
    Self::Opposite: ParamDirection<S>,
{
    fn lower_closure_parameter(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        closure: ClosureSource,
    ) -> Result<ClosureParameter<S, Self>, LowerError>;

    fn lower_closure_return(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        closure: ClosureSource,
    ) -> Result<ClosureReturn<S, Self>, LowerError>;

    fn lower_closure_param(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        name: CanonicalName,
        meta: ElementMeta,
        closure: ClosureSource,
    ) -> Result<ParamDecl<S, Self>, LowerError>;
}

impl<S: SurfaceLower> LowerClosure<S> for IntoRust {
    fn lower_closure_parameter(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        closure: ClosureSource,
    ) -> Result<ClosureParameter<S, IntoRust>, LowerError> {
        super::lower_closure_param_into_rust::<S>(index, ids, allocator, closure)
    }

    fn lower_closure_return(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        closure: ClosureSource,
    ) -> Result<ClosureReturn<S, IntoRust>, LowerError> {
        super::lower_closure_return_into_rust::<S>(index, ids, allocator, closure)
    }

    fn lower_closure_param(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        name: CanonicalName,
        meta: ElementMeta,
        closure: ClosureSource,
    ) -> Result<ParamDecl<S, IntoRust>, LowerError> {
        let param = Self::lower_closure_parameter(index, ids, allocator, closure)?;
        Ok(<ParamDecl<S, IntoRust>>::closure(name, meta, param))
    }
}

impl<S: SurfaceLower> LowerClosure<S> for OutOfRust {
    fn lower_closure_parameter(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        closure: ClosureSource,
    ) -> Result<ClosureParameter<S, OutOfRust>, LowerError> {
        super::lower_closure_param_out_of_rust::<S>(index, ids, allocator, closure)
    }

    fn lower_closure_return(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        closure: ClosureSource,
    ) -> Result<ClosureReturn<S, OutOfRust>, LowerError> {
        super::lower_closure_return_out_of_rust::<S>(index, ids, allocator, closure)
    }

    fn lower_closure_param(
        index: &Index,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        name: CanonicalName,
        meta: ElementMeta,
        closure: ClosureSource,
    ) -> Result<ParamDecl<S, OutOfRust>, LowerError> {
        let param = Self::lower_closure_parameter(index, ids, allocator, closure)?;
        Ok(<ParamDecl<S, OutOfRust>>::closure(name, meta, param))
    }
}
