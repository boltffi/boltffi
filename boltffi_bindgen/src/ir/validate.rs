use crate::ir::contract::{FfiContract, TypeCatalog};
use crate::ir::definitions::{
    EnumRepr, ParamDef, ParamPassing, ReturnDef, StreamDef, VariantPayload,
};
use crate::ir::ids::{ClassId, EnumId, FieldName, FunctionId, ParamName, StreamId, VariantName};
use crate::ir::types::TypeExpr;

#[derive(Debug, Clone)]
pub enum ValidationError {
    UnresolvedType {
        context: String,
        error: String,
    },
    InvalidParamPassing {
        context: String,
        message: String,
    },
    InvalidPrimitive {
        context: String,
        message: String,
    },
    NonEncodableInData {
        context: String,
        message: String,
    },
    InvalidStreamItemType {
        class_id: ClassId,
        stream_id: StreamId,
        reason: String,
    },
}

pub fn validate_contract(contract: &FfiContract) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let catalog = &contract.catalog;

    catalog.all_records().for_each(|record| {
        record.fields.iter().for_each(|field| {
            validate_field_type(
                &field.type_expr,
                catalog,
                &record.id,
                &field.name,
                &mut errors,
            );
        });
    });

    catalog
        .all_enums()
        .filter_map(|e| match &e.repr {
            EnumRepr::Data { variants, .. } => Some((e, variants)),
            _ => None,
        })
        .for_each(|(enumeration, variants)| {
            variants.iter().for_each(|variant| {
                validate_variant_payload(
                    &variant.payload,
                    catalog,
                    &enumeration.id,
                    &variant.name,
                    &mut errors,
                );
            });
        });

    contract.functions.iter().for_each(|func| {
        validate_callable(&func.id, &func.params, &func.returns, catalog, &mut errors);
    });

    catalog.all_classes().for_each(|class| {
        class.constructors.iter().for_each(|ctor| {
            let ctx = format!(
                "{}::{}",
                class.id,
                ctor.name().map(|n| n.as_str()).unwrap_or("new")
            );
            ctor.params().into_iter().for_each(|param| {
                validate_param_type(
                    &param.type_expr,
                    &param.passing,
                    catalog,
                    &ctx,
                    &param.name,
                    &mut errors,
                );
            });
        });

        class.methods.iter().for_each(|method| {
            let ctx = format!("{}::{}", class.id, method.id);
            validate_callable_inner(&ctx, &method.params, &method.returns, catalog, &mut errors);
        });

        class.streams.iter().for_each(|stream| {
            validate_stream_item_type(&class.id, stream, catalog, &mut errors);
        });
    });

    catalog.all_callbacks().for_each(|callback| {
        callback.methods.iter().for_each(|method| {
            let ctx = format!("{}::{}", callback.id, method.id);
            validate_callable_inner(&ctx, &method.params, &method.returns, catalog, &mut errors);
        });
    });

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_callable(
    id: &FunctionId,
    params: &[ParamDef],
    returns: &ReturnDef,
    catalog: &TypeCatalog,
    errors: &mut Vec<ValidationError>,
) {
    let ctx = id.as_str();
    validate_callable_inner(ctx, params, returns, catalog, errors);
}

fn validate_callable_inner(
    ctx: &str,
    params: &[ParamDef],
    returns: &ReturnDef,
    catalog: &TypeCatalog,
    errors: &mut Vec<ValidationError>,
) {
    params.iter().for_each(|param| {
        validate_param_type(
            &param.type_expr,
            &param.passing,
            catalog,
            ctx,
            &param.name,
            errors,
        );
    });
    validate_return_type(returns, catalog, ctx, errors);
}

fn validate_param_type(
    expr: &TypeExpr,
    passing: &ParamPassing,
    catalog: &TypeCatalog,
    ctx: &str,
    param_name: &ParamName,
    errors: &mut Vec<ValidationError>,
) {
    if let Err(e) = validate_type_expr(expr, catalog) {
        errors.push(ValidationError::UnresolvedType {
            context: format!("{}::{}", ctx, param_name),
            error: e,
        });
    }
    if let Err(e) = validate_param_passing(expr, passing, param_name) {
        errors.push(e);
    }
    if let Err(e) = reject_non_encodable_in_param(expr, &format!("{}::{}", ctx, param_name)) {
        errors.push(e);
    }
}

fn reject_non_encodable_in_param(expr: &TypeExpr, context: &str) -> Result<(), ValidationError> {
    match expr {
        TypeExpr::Option(inner) => {
            if matches!(inner.as_ref(), TypeExpr::Handle(_) | TypeExpr::Callback(_)) {
                Ok(())
            } else {
                reject_non_encodable_nested(inner, context)
            }
        }
        TypeExpr::Vec(inner) => {
            if matches!(inner.as_ref(), TypeExpr::Handle(_) | TypeExpr::Callback(_)) {
                Err(ValidationError::NonEncodableInData {
                    context: context.to_string(),
                    message: "Vec<Handle>/Vec<Callback> cannot be encoded".to_string(),
                })
            } else {
                reject_non_encodable_nested(inner, context)
            }
        }
        TypeExpr::Result { ok, err } => {
            reject_non_encodable_in_param(ok, context)?;
            reject_non_encodable_in_param(err, context)
        }
        _ => Ok(()),
    }
}

fn reject_non_encodable_nested(expr: &TypeExpr, context: &str) -> Result<(), ValidationError> {
    match expr {
        TypeExpr::Handle(_) => Err(ValidationError::NonEncodableInData {
            context: context.to_string(),
            message: "Handle cannot be nested inside Vec/Option/Result".to_string(),
        }),
        TypeExpr::Callback(_) => Err(ValidationError::NonEncodableInData {
            context: context.to_string(),
            message: "Callback cannot be nested inside Vec/Option/Result".to_string(),
        }),
        TypeExpr::Vec(inner) | TypeExpr::Option(inner) => {
            reject_non_encodable_nested(inner, context)
        }
        TypeExpr::Result { ok, err } => {
            reject_non_encodable_nested(ok, context)?;
            reject_non_encodable_nested(err, context)
        }
        _ => Ok(()),
    }
}

fn validate_return_type(
    returns: &ReturnDef,
    catalog: &TypeCatalog,
    ctx: &str,
    errors: &mut Vec<ValidationError>,
) {
    match returns {
        ReturnDef::Void => {}
        ReturnDef::Value(ty) => {
            let return_ctx = format!("{} return", ctx);
            if let Err(e) = validate_type_expr(ty, catalog) {
                errors.push(ValidationError::UnresolvedType {
                    context: return_ctx.clone(),
                    error: e,
                });
            }
            if let Err(e) = reject_non_encodable_in_param(ty, &return_ctx) {
                errors.push(e);
            }
        }
        ReturnDef::Result { ok, err } => {
            let ok_ctx = format!("{} return (ok)", ctx);
            let err_ctx = format!("{} return (err)", ctx);
            if let Err(e) = validate_type_expr(ok, catalog) {
                errors.push(ValidationError::UnresolvedType {
                    context: ok_ctx.clone(),
                    error: e,
                });
            }
            if let Err(e) = reject_non_encodable_in_param(ok, &ok_ctx) {
                errors.push(e);
            }
            if let Err(e) = validate_type_expr(err, catalog) {
                errors.push(ValidationError::UnresolvedType {
                    context: err_ctx.clone(),
                    error: e,
                });
            }
            if let Err(e) = reject_non_encodable_in_param(err, &err_ctx) {
                errors.push(e);
            }
        }
    }
}

fn validate_variant_payload(
    payload: &VariantPayload,
    catalog: &TypeCatalog,
    enum_id: &EnumId,
    variant_name: &VariantName,
    errors: &mut Vec<ValidationError>,
) {
    match payload {
        VariantPayload::Unit => {}
        VariantPayload::Tuple(types) => {
            types.iter().enumerate().for_each(|(idx, type_expr)| {
                let ctx = format!("{}::{}::{}", enum_id, variant_name, idx);
                if let Err(e) = validate_type_expr(type_expr, catalog) {
                    errors.push(ValidationError::UnresolvedType {
                        context: ctx.clone(),
                        error: e,
                    });
                }
                if let Err(e) = reject_non_encodable_in_data(type_expr, &ctx) {
                    errors.push(e);
                }
            });
        }
        VariantPayload::Struct(fields) => {
            let parent = format!("{}::{}", enum_id, variant_name);
            fields.iter().for_each(|field| {
                validate_field_type(&field.type_expr, catalog, &parent, &field.name, errors);
            });
        }
    }
}

fn validate_field_type(
    expr: &TypeExpr,
    catalog: &TypeCatalog,
    parent_id: impl std::fmt::Display,
    field_name: &FieldName,
    errors: &mut Vec<ValidationError>,
) {
    if let Err(e) = validate_type_expr(expr, catalog) {
        errors.push(ValidationError::UnresolvedType {
            context: format!("{}::{}", parent_id, field_name),
            error: e,
        });
    }
    if let Err(e) = reject_non_encodable_in_data(expr, &format!("{}::{}", parent_id, field_name)) {
        errors.push(e);
    }
}

fn validate_type_expr(expr: &TypeExpr, catalog: &TypeCatalog) -> Result<(), String> {
    match expr {
        TypeExpr::Void | TypeExpr::Primitive(_) | TypeExpr::String | TypeExpr::Str => Ok(()),
        TypeExpr::Record(id) => catalog
            .resolve_record(id)
            .map(|_| ())
            .ok_or_else(|| format!("unresolved record: {}", id)),
        TypeExpr::Enum(id) => catalog
            .resolve_enum(id)
            .map(|_| ())
            .ok_or_else(|| format!("unresolved enum: {}", id)),
        TypeExpr::Callback(id) => catalog
            .resolve_callback(id)
            .map(|_| ())
            .ok_or_else(|| format!("unresolved callback: {}", id)),
        TypeExpr::Custom(id) => catalog
            .resolve_custom(id)
            .map(|_| ())
            .ok_or_else(|| format!("unresolved custom type: {}", id)),
        TypeExpr::Builtin(id) => catalog
            .resolve_builtin(id)
            .map(|_| ())
            .ok_or_else(|| format!("unresolved builtin: {:?}", id)),
        TypeExpr::Handle(id) => catalog
            .resolve_class(id)
            .map(|_| ())
            .ok_or_else(|| format!("unresolved class handle: {}", id)),
        TypeExpr::Vec(inner) | TypeExpr::Option(inner) => validate_type_expr(inner, catalog),
        TypeExpr::Result { ok, err } => {
            validate_type_expr(ok, catalog)?;
            validate_type_expr(err, catalog)
        }
    }
}

fn reject_non_encodable_in_data(expr: &TypeExpr, context: &str) -> Result<(), ValidationError> {
    match expr {
        TypeExpr::Handle(_) => Err(ValidationError::NonEncodableInData {
            context: context.to_string(),
            message: "Handle (class reference) cannot appear inside records/enums - it's an opaque pointer, not serializable".to_string(),
        }),
        TypeExpr::Callback(_) => Err(ValidationError::NonEncodableInData {
            context: context.to_string(),
            message: "Callback trait cannot appear inside records/enums - use as function parameter only".to_string(),
        }),
        TypeExpr::Vec(inner) | TypeExpr::Option(inner) => {
            reject_non_encodable_in_data(inner, context)
        }
        TypeExpr::Result { ok, err } => {
            reject_non_encodable_in_data(ok, context)?;
            reject_non_encodable_in_data(err, context)
        }
        _ => Ok(()),
    }
}

fn validate_param_passing(
    expr: &TypeExpr,
    passing: &ParamPassing,
    param_name: &ParamName,
) -> Result<(), ValidationError> {
    match (passing, expr) {
        (ParamPassing::ImplTrait, TypeExpr::Callback(_)) => Ok(()),
        (ParamPassing::BoxedDyn, TypeExpr::Callback(_)) => Ok(()),
        (ParamPassing::ImplTrait | ParamPassing::BoxedDyn, _) => {
            Err(ValidationError::InvalidParamPassing {
                context: param_name.to_string(),
                message: "impl Trait or Box<dyn Trait> requires a callback trait".to_string(),
            })
        }
        _ => Ok(()),
    }
}

fn validate_stream_item_type(
    class_id: &ClassId,
    stream: &StreamDef,
    catalog: &TypeCatalog,
    errors: &mut Vec<ValidationError>,
) {
    let ctx = format!("{}::{}::item", class_id, stream.id);
    if let Err(e) = validate_type_expr(&stream.item_type, catalog) {
        errors.push(ValidationError::UnresolvedType {
            context: ctx.clone(),
            error: e,
        });
    }
    if !is_wire_encodable(&stream.item_type) {
        errors.push(ValidationError::InvalidStreamItemType {
            class_id: class_id.clone(),
            stream_id: stream.id.clone(),
            reason: "stream items must be wire-encodable (no Handle or Callback)".to_string(),
        });
    }
}

fn is_wire_encodable(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Void
        | TypeExpr::Primitive(_)
        | TypeExpr::String
        | TypeExpr::Str
        | TypeExpr::Record(_)
        | TypeExpr::Enum(_)
        | TypeExpr::Custom(_)
        | TypeExpr::Builtin(_) => true,
        TypeExpr::Option(inner) | TypeExpr::Vec(inner) => is_wire_encodable(inner),
        TypeExpr::Result { ok, err } => is_wire_encodable(ok) && is_wire_encodable(err),
        TypeExpr::Handle(_) | TypeExpr::Callback(_) => false,
    }
}
