use boltffi_ast::{ReturnDef, TypeExpr};

use crate::{
    DirectValueType, Direction, ElementMeta, ErrorDecl, HandleTarget, ParamDirection, Primitive,
    ReturnDecl, ReturnPlan, ValueRef,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, records,
    surface::SurfaceLower, symbol::SymbolAllocator, types,
};

use super::{
    CallableOwner, CallbackHandleSource, ClassHandleSource, ClosureSource, ValueSpecialization,
    params::LowerClosure, substitute_self_type,
};

/// The return and error pair produced by [`lower`] for one source
/// [`ReturnDef`].
pub type Lowered<S, D> = (ReturnDecl<S, D>, ErrorDecl<S, D>);

/// Lowers a source [`ReturnDef`] into the pair the enclosing
/// [`CallableDecl`](crate::CallableDecl) records: a
/// [`ReturnDecl<S, D>`] for the success value and an
/// [`ErrorDecl<S, D>`] for the failure channel.
///
/// `D` is the enclosing scope's `K::ReturnDirection`. A `Result<T, E>`
/// return spills the success value into the out-pointer slot through
/// [`ReturnPlan::into_out`] and routes the error status through the
/// return slot. A `Result<(), E>` return produces a void success
/// channel paired with an encoded error channel.
///
/// Closure returns dispatch through [`LowerClosure`] (the trait
/// also covers the return position because the closure crossing shape
/// is the same in either slot), so the direction `D` decides
/// structurally whether the invoke contract is foreign- or
/// Rust-implemented.
pub fn lower<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner,
    root_encoding: codecs::RootEncoding,
    return_def: &ReturnDef,
) -> Result<Lowered<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    match return_def {
        ReturnDef::Void => Ok((
            ReturnDecl::new(ElementMeta::new(None, None, None), ReturnPlan::Void),
            ErrorDecl::none(),
        )),
        ReturnDef::Value(type_expr) => {
            if let TypeExpr::Result { ok, err } = type_expr {
                let ok_type_expr = substitute_self_type(owner, ok)?;
                let err_type_expr = substitute_self_type(owner, err)?;
                let success =
                    lower_return_plan::<S, D>(index, ids, allocator, root_encoding, &ok_type_expr)?
                        .into_out();
                let error = lower_error::<S, D>(index, ids, &err_type_expr)?;
                return Ok((
                    ReturnDecl::new(ElementMeta::new(None, None, None), success),
                    error,
                ));
            }
            if matches!(type_expr, TypeExpr::Unit) {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::UnitInValuePosition,
                ));
            }
            let type_expr = substitute_self_type(owner, type_expr)?;
            let plan =
                lower_plain_return::<S, D>(index, ids, allocator, root_encoding, &type_expr)?;
            Ok((
                ReturnDecl::new(ElementMeta::new(None, None, None), plan),
                ErrorDecl::none(),
            ))
        }
    }
}

fn lower_plain_return<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    root_encoding: codecs::RootEncoding,
    type_expr: &TypeExpr,
) -> Result<ReturnPlan<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    match specialize_return::<S, D>(index, ids, type_expr)? {
        Some(plan) => Ok(plan),
        None => lower_return_plan::<S, D>(index, ids, allocator, root_encoding, type_expr),
    }
}

fn specialize_return<S: SurfaceLower, D: Direction>(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<Option<ReturnPlan<S, D>>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    let specialization = ValueSpecialization::from_type_expr::<S>(index, ids, type_expr)?;
    Ok(match specialization {
        Some(ValueSpecialization::ScalarOption(primitive)) => {
            Some(ReturnPlan::ScalarOptionViaReturnSlot { primitive })
        }
        Some(ValueSpecialization::DirectVector(element)) => {
            Some(ReturnPlan::DirectVecViaReturnSlot { element })
        }
        None => None,
    })
}

fn lower_error<S: SurfaceLower, D: Direction>(
    index: &Index,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<ErrorDecl<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    let ty = types::lower(ids, type_expr)?;
    let codec_node = codecs::node(index, ids, type_expr, ValueRef::self_value())?;
    Ok(ErrorDecl::EncodedViaReturnSlot {
        ty,
        codec: D::make_codec(ValueRef::self_value(), codec_node),
        shape: S::encoded_return_shape(),
    })
}

fn lower_return_plan<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    index: &Index,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    root_encoding: codecs::RootEncoding,
    type_expr: &TypeExpr,
) -> Result<ReturnPlan<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    if let Some(handle) = ClassHandleSource::from_type_expr(type_expr) {
        return Ok(ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(ids.class(handle.id)?),
            carrier: S::class_handle_carrier(),
            presence: handle.presence,
        });
    }
    if let Some(handle) = CallbackHandleSource::from_type_expr(type_expr) {
        return Ok(ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Callback(ids.callback(handle.id)?),
            carrier: S::callback_handle_carrier(),
            presence: handle.presence,
        });
    }
    if let Some(closure) = ClosureSource::from_type_expr(type_expr) {
        let closure_return = D::lower_closure_return(index, ids, allocator, closure)?;
        return Ok(ReturnPlan::ClosureViaOutPointer(closure_return));
    }
    match type_expr {
        TypeExpr::Unit => Ok(ReturnPlan::Void),
        TypeExpr::Primitive(primitive) => Ok(ReturnPlan::DirectViaReturnSlot {
            ty: DirectValueType::primitive(Primitive::from(*primitive)),
        }),
        TypeExpr::Record { id, .. } if index.record(id).is_some_and(records::is_direct) => {
            let ty = DirectValueType::record(ids.record(id)?);
            Ok(match S::direct_record_return_slot() {
                crate::ReturnValueSlot::ReturnSlot => ReturnPlan::DirectViaReturnSlot { ty },
                crate::ReturnValueSlot::OutPointer => ReturnPlan::DirectViaOutPointer { ty },
            })
        }
        TypeExpr::Enum { id, .. } if index.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(ReturnPlan::DirectViaReturnSlot {
                ty: DirectValueType::enumeration(ids.enumeration(id)?),
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
            let codec_node =
                root_encoding.node::<S>(index, ids, type_expr, ValueRef::self_value())?;
            Ok(ReturnPlan::EncodedViaReturnSlot {
                ty,
                codec: D::make_codec(ValueRef::self_value(), codec_node),
                shape: S::encoded_return_shape(),
            })
        }
        TypeExpr::SelfType
        | TypeExpr::Parameter(_)
        | TypeExpr::Class { .. }
        | TypeExpr::FnPtr(_)
        | TypeExpr::ImplTrait(_)
        | TypeExpr::Dyn(_)
        | TypeExpr::Boxed(_)
        | TypeExpr::Arc(_) => {
            Err(types::lower(ids, type_expr).expect_err(
                "return value-plan lowering reached a source type reserved for handle, closure, owner-substitution, or generic rejection before the direct/encoded fallback",
            ))
        }
    }
}
