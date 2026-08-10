use boltffi_ast::{ReturnDef, TypeExpr};

use crate::{
    DirectValueType, Direction, ElementMeta, ErrorDecl, HandleTarget, ParamDirection, Primitive,
    ReturnDecl, ReturnPlan, ValueRef,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, opaque,
    records, surface::SurfaceLower, symbol::SymbolAllocator, types,
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
                if opaque::contains(index, &ok_type_expr) {
                    return Err(LowerError::unsupported_type(
                        UnsupportedType::NativeOpaqueRecordResult,
                    ));
                }
                let err_type_expr = substitute_self_type(owner, err)?;
                // Reject opaque records in the error arm as well
                // (e.g. Result<T, Opaque> or Result<T, Option<Opaque>>).
                if opaque::contains(index, &err_type_expr) {
                    return Err(LowerError::unsupported_type(
                        UnsupportedType::NativeOpaqueRecordResult,
                    ));
                }
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
            // Reject nested native opaque records (Option<Opaque>, Vec<Opaque>, named
            // records containing opaque fields, etc.) after Self-type substitution so that a
            // method returning Option<Self> on a native-opaque record is also caught. Only an
            // exact top-level native opaque record is admitted to lower_plain_return.
            let is_direct_opaque_record = matches!(
                &type_expr,
                TypeExpr::Record { id, .. }
                    if index.record(id).is_some_and(|record| {
                        record.encoding == boltffi_ast::RecordEncoding::NativeOpaque
                    })
            );
            if !is_direct_opaque_record && opaque::contains(index, &type_expr) {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::NativeOpaqueRecordNestedReturn,
                ));
            }
            let plan =
                lower_plain_return::<S, D>(index, ids, allocator, root_encoding, &type_expr)?;
            // Native opaque records are only valid as synchronous, infallible,
            // exact top-level free-function returns. Provide a distinct error
            // variant for callback/trait positions to give clearer diagnostics.
            if matches!(plan, ReturnPlan::NativeOpaqueRecord { .. }) {
                let error = match owner {
                    CallableOwner::Function => None,
                    CallableOwner::Trait(_) => Some(UnsupportedType::NativeOpaqueRecordInCallback),
                    _ => Some(UnsupportedType::NativeOpaqueRecordMethod),
                };
                if let Some(kind) = error {
                    return Err(LowerError::unsupported_type(kind));
                }
            }
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
    let specialization = ValueSpecialization::from_return::<S, D>(index, ids, type_expr)?;
    Ok(match specialization {
        Some(ValueSpecialization::ScalarOption(primitive, enum_target)) => {
            Some(ReturnPlan::ScalarOptionViaReturnSlot {
                primitive,
                enum_target,
            })
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
        TypeExpr::Record { id, .. }
            if index.record(id).is_some_and(|r| {
                r.encoding == boltffi_ast::RecordEncoding::NativeOpaque
            }) =>
        {
            Ok(ReturnPlan::NativeOpaqueRecord {
                record: ids.record(id)?,
            })
        }
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

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, FieldDef, PackageInfo, Path, Primitive as SourcePrimitive, RecordDef,
        RecordEncoding, ReturnDef, SourceContract, TypeExpr,
    };

    use crate::{LowerErrorKind, Native, OutOfRust, UnsupportedType};

    use super::*;

    fn name(value: &str) -> CanonicalName {
        CanonicalName::single(value)
    }

    fn opaque_record() -> RecordDef {
        let mut record = RecordDef::new("demo::Opaque".into(), name("Opaque"));
        record.encoding = RecordEncoding::NativeOpaque;
        record
    }

    fn opaque_type() -> TypeExpr {
        TypeExpr::record("demo::Opaque".into(), Path::single("Opaque"))
    }

    fn lower_return(
        source: &SourceContract,
        owner: CallableOwner,
        returns: ReturnDef,
    ) -> Result<Lowered<Native, OutOfRust>, LowerError> {
        let index = Index::new(source);
        let ids = DeclarationIds::from_source(source).expect("source ids");
        lower::<Native, OutOfRust>(
            &index,
            &ids,
            &mut SymbolAllocator::new(),
            owner,
            codecs::RootEncoding::Surface,
            &returns,
        )
    }

    #[test]
    fn result_ok_opaque_is_rejected_as_native_opaque_record_result() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(opaque_record());
        let error = lower_return(
            &source,
            CallableOwner::Function,
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(opaque_type()),
                err: Box::new(TypeExpr::String),
            }),
        )
        .expect_err("Result<Opaque, E> must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordResult)
        ));
    }

    #[test]
    fn result_error_opaque_is_rejected_as_native_opaque_record_result() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(opaque_record());
        let error = lower_return(
            &source,
            CallableOwner::Function,
            ReturnDef::value(TypeExpr::Result {
                ok: Box::new(TypeExpr::Primitive(SourcePrimitive::U32)),
                err: Box::new(opaque_type()),
            }),
        )
        .expect_err("Result<T, Opaque> must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordResult)
        ));
    }

    #[test]
    fn named_envelope_returning_opaque_is_rejected_before_record_lowering() {
        let mut envelope = RecordDef::new("demo::Envelope".into(), name("Envelope"));
        envelope
            .fields
            .push(FieldDef::new(name("opaque"), opaque_type()));
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records = vec![opaque_record(), envelope];
        let error = lower_return(
            &source,
            CallableOwner::Function,
            ReturnDef::value(TypeExpr::record(
                "demo::Envelope".into(),
                Path::single("Envelope"),
            )),
        )
        .expect_err("named opaque envelope must reject before record lowering");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordNestedReturn)
        ));
    }

    #[test]
    fn trait_owner_direct_opaque_return_is_rejected_as_callback() {
        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.records.push(opaque_record());
        let owner_trait = boltffi_ast::TraitDef::new("demo::Sink".into(), name("Sink"));
        let error = lower_return(
            &source,
            CallableOwner::Trait(&owner_trait),
            ReturnDef::value(opaque_type()),
        )
        .expect_err("trait opaque return must reject as callback");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::NativeOpaqueRecordInCallback)
        ));
    }
}
