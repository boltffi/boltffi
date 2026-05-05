use std::collections::HashSet;

use boltffi_ffi_rules::naming;

use crate::ir::abi::{AbiCall, AsyncCall, CallMode};
use crate::ir::definitions::{FunctionDef, ReturnDef};
use crate::ir::ops::{ReadOp, ReadSeq};
use crate::ir::types::TypeExpr;

use super::super::ast::{
    CSharpClassName, CSharpComment, CSharpExpression, CSharpIdentity, CSharpLocalName,
    CSharpMethodName, CSharpType,
};
use super::super::plan::{
    CSharpAsyncCallPlan, CSharpFunctionPlan, CSharpParamPlan, CSharpReturnKind,
};
use super::decode;
use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Lowers a Rust function definition to a [`CSharpFunctionPlan`].
    /// Returns `None` if any param/return type isn't yet supported by the
    /// C# backend.
    pub(super) fn lower_function(&self, function: &FunctionDef) -> Option<CSharpFunctionPlan> {
        if !function.params.iter().all(|p| self.is_supported_param(p)) {
            return None;
        }

        let return_type = self.lower_return(&function.returns)?;
        let call = self.abi_call_for_function(function)?;
        let complete_decode_ops = match &call.mode {
            CallMode::Sync => call.returns.decode_ops.as_ref(),
            CallMode::Async(async_call) => async_call.result.decode_ops.as_ref(),
        };
        let return_kind =
            self.return_kind(&function.returns, &return_type, complete_decode_ops, None);

        let wire_writers = self.wire_writers_for_params(function)?;

        let params: Vec<CSharpParamPlan> = function
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;

        let name = (&function.id).into();
        let async_call = self.async_call_from_mode(call, &name);

        Some(CSharpFunctionPlan {
            summary_doc: CSharpComment::from_str_option(function.doc.as_deref()),
            name,
            ffi_name: naming::function_ffi_name(function.id.as_str()).into(),
            async_call,
            params,
            return_type,
            return_kind,
            wire_writers,
        })
    }

    pub(super) fn async_call_from_mode(
        &self,
        call: &AbiCall,
        native_method_name: &CSharpMethodName,
    ) -> Option<CSharpAsyncCallPlan> {
        let CallMode::Async(async_call) = &call.mode else {
            return None;
        };
        Some(csharp_async_call_plan(async_call, native_method_name))
    }

    /// Selects the [`CSharpReturnKind`] and pre-renders the inner decode
    /// expressions for the encoded-Vec and Option shapes. `shadowed` is the
    /// set of class names shadowed in the surrounding scope (used to qualify
    /// type references); pass `None` for top-level functions.
    pub(super) fn return_kind(
        &self,
        return_def: &ReturnDef,
        return_type: &CSharpType,
        decode_ops: Option<&ReadSeq>,
        shadowed: Option<&HashSet<CSharpClassName>>,
    ) -> CSharpReturnKind {
        if let ReturnDef::Result { .. } = return_def {
            return self.result_return_kind(return_def, decode_ops, shadowed);
        }
        if return_type.is_void() {
            return CSharpReturnKind::Void;
        }
        let raw_type = match return_def {
            ReturnDef::Value(t) => t,
            _ => return CSharpReturnKind::Direct,
        };
        // Custom returns always cross as wire-encoded FfiBuf (the macro
        // wraps the underlying value uniformly). For repr shapes that
        // already have a wire-decode path (String, Record, Enum, Vec,
        // Option) the normalized dispatch below produces the right
        // kind; for Custom<Primitive> the dispatch would otherwise fall
        // through to Direct, so synthesize a single-op wire decode.
        let is_custom = matches!(raw_type, TypeExpr::Custom(_));
        let normalized = self.normalize_custom_type_expr(raw_type);
        if is_custom && matches!(normalized, TypeExpr::Primitive(_)) {
            let decode_seq = decode_ops.expect("Custom return must carry decode_ops");
            let mut locals = decode::DecodeLocalCounters::default();
            let reader =
                CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("reader")));
            let decode_expr = decode::lower_decode_expr(
                decode_seq,
                &reader,
                shadowed,
                &self.namespace,
                &mut locals,
            );
            return CSharpReturnKind::WireDecodeValue { decode_expr };
        }
        // The macro emits `Vec<Custom<_>>` returns as wire-encoded
        // (length-prefixed) regardless of repr — Custom is treated as
        // opaque on the return path, so even `Vec<Custom<i64>>` ships
        // as `[len][i64][i64]...` rather than the raw blittable layout
        // a bare `Vec<i64>` would use. Force the encoded-array path
        // (length-prefix + per-element decode) instead of the
        // top-level blittable shortcut the normalized dispatch below
        // would otherwise pick.
        if let TypeExpr::Vec(raw_inner) = raw_type
            && matches!(raw_inner.as_ref(), TypeExpr::Custom(_))
        {
            let normalized_inner = self.normalize_custom_type_expr(raw_inner);
            let element_seq = vec_element_read_seq(decode_ops)
                .expect("Vec<Custom> return must carry decode_ops with a Vec ReadOp");
            let mut locals = decode::DecodeLocalCounters::default();
            let closure_var = locals.next_closure_var();
            let closure_receiver =
                CSharpExpression::Identity(CSharpIdentity::Local(closure_var.clone()));
            let body = decode::lower_decode_expr(
                &element_seq,
                &closure_receiver,
                shadowed,
                &self.namespace,
                &mut locals,
            );
            return CSharpReturnKind::WireDecodeEncodedArray {
                element_type: CSharpType::from_type_expr(&normalized_inner)
                    .qualify_if_shadowed_opt(shadowed, &self.namespace),
                decode_lambda: CSharpExpression::Lambda {
                    param: closure_var,
                    body: Box::new(body),
                },
            };
        }
        match &normalized {
            TypeExpr::String => CSharpReturnKind::WireDecodeString,
            TypeExpr::Record(id) if !self.is_blittable_record(id) => {
                CSharpReturnKind::WireDecodeObject {
                    class_name: id.into(),
                }
            }
            TypeExpr::Enum(id) if self.is_data_enum(id) => CSharpReturnKind::WireDecodeObject {
                class_name: id.into(),
            },
            TypeExpr::Vec(inner) => match inner.as_ref() {
                TypeExpr::Primitive(p) => CSharpReturnKind::WireDecodeBlittablePrimitiveArray {
                    method: decode::top_level_blittable_primitive_array_method(*p),
                    type_arg: decode::top_level_blittable_primitive_array_type_arg(*p),
                },
                TypeExpr::Record(id) if self.is_blittable_record(id) => {
                    CSharpReturnKind::WireDecodeBlittableRecordArray { element: id.into() }
                }
                _ => {
                    let element_seq = vec_element_read_seq(decode_ops)
                        .expect("encoded Vec return must carry decode_ops with a Vec ReadOp");
                    let mut locals = decode::DecodeLocalCounters::default();
                    let closure_var = locals.next_closure_var();
                    let closure_receiver =
                        CSharpExpression::Identity(CSharpIdentity::Local(closure_var.clone()));
                    let body = decode::lower_decode_expr(
                        &element_seq,
                        &closure_receiver,
                        shadowed,
                        &self.namespace,
                        &mut locals,
                    );
                    CSharpReturnKind::WireDecodeEncodedArray {
                        element_type: CSharpType::from_type_expr(inner)
                            .qualify_if_shadowed_opt(shadowed, &self.namespace),
                        decode_lambda: CSharpExpression::Lambda {
                            param: closure_var,
                            body: Box::new(body),
                        },
                    }
                }
            },
            TypeExpr::Option(_) => {
                let decode_seq = decode_ops.expect("Option return must carry decode_ops");
                let mut locals = decode::DecodeLocalCounters::default();
                let reader = CSharpExpression::Identity(CSharpIdentity::Local(
                    CSharpLocalName::new("reader"),
                ));
                let decode_expr = decode::lower_decode_expr(
                    decode_seq,
                    &reader,
                    shadowed,
                    &self.namespace,
                    &mut locals,
                );
                CSharpReturnKind::WireDecodeOption { decode_expr }
            }
            // Primitives, bools, blittable records, and C-style enums
            // are all direct: the CLR marshals them across P/Invoke
            // without any wrapper help.
            _ => CSharpReturnKind::Direct,
        }
    }
}

pub(super) fn csharp_async_call_plan(
    async_call: &AsyncCall,
    native_method_name: &CSharpMethodName,
) -> CSharpAsyncCallPlan {
    CSharpAsyncCallPlan {
        poll_ffi_name: (&async_call.poll).into(),
        complete_ffi_name: (&async_call.complete).into(),
        cancel_ffi_name: (&async_call.cancel).into(),
        free_ffi_name: (&async_call.free).into(),
        poll_method_name: CSharpMethodName::new(format!("{native_method_name}Poll")),
        complete_method_name: CSharpMethodName::new(format!("{native_method_name}Complete")),
        cancel_method_name: CSharpMethodName::new(format!("{native_method_name}Cancel")),
        free_method_name: CSharpMethodName::new(format!("{native_method_name}FreeFuture")),
    }
}

/// Extracts the per-element [`ReadSeq`] from a Vec's top-level
/// [`ReadSeq`]. Used to render the inner decode of
/// `ReadEncodedArray<T>(r => ...)`. Primitive-element Vec returns
/// short-circuit through dedicated `Read{Type}Array` methods and
/// never call this.
fn vec_element_read_seq(decode_ops: Option<&ReadSeq>) -> Option<ReadSeq> {
    let decode = decode_ops?;
    match decode.ops.first()? {
        ReadOp::Vec { element, .. } => Some((**element).clone()),
        _ => None,
    }
}
