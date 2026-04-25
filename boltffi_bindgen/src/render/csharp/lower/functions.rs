use std::collections::HashSet;

use boltffi_ffi_rules::naming;

use crate::ir::definitions::{FunctionDef, ReturnDef};
use crate::ir::ops::{ReadOp, ReadSeq};
use crate::ir::types::TypeExpr;

use super::super::ast::{
    CSharpClassName, CSharpExpression, CSharpIdentity, CSharpLocalName, CSharpType,
};
use super::super::plan::{CSharpFunctionPlan, CSharpParamPlan, CSharpReturnKind};
use super::decode;
use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Lowers a Rust function definition to a [`CSharpFunctionPlan`].
    /// Returns `None` if the function is async or any param/return type
    /// isn't yet supported by the C# backend.
    pub(super) fn lower_function(&self, function: &FunctionDef) -> Option<CSharpFunctionPlan> {
        if function.is_async() {
            return None;
        }

        if !function.params.iter().all(|p| self.is_supported_param(p)) {
            return None;
        }

        let return_type = self.lower_return(&function.returns)?;
        let call = self.abi_call_for_function(function)?;
        let return_kind = self.return_kind(
            &function.returns,
            &return_type,
            call.returns.decode_ops.as_ref(),
            None,
        );

        let wire_writers = self.wire_writers_for_params(function)?;

        let params: Vec<CSharpParamPlan> = function
            .params
            .iter()
            .map(|p| self.lower_param(p, &wire_writers))
            .collect::<Option<Vec<_>>>()?;

        Some(CSharpFunctionPlan {
            name: (&function.id).into(),
            ffi_name: naming::function_ffi_name(function.id.as_str()).into(),
            params,
            return_type,
            return_kind,
            wire_writers,
        })
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
        if return_type.is_void() {
            return CSharpReturnKind::Void;
        }
        match return_def {
            ReturnDef::Value(TypeExpr::String) => CSharpReturnKind::WireDecodeString,
            ReturnDef::Value(TypeExpr::Record(id)) if !self.is_blittable_record(id) => {
                CSharpReturnKind::WireDecodeObject {
                    class_name: id.into(),
                }
            }
            ReturnDef::Value(TypeExpr::Enum(id)) if self.is_data_enum(id) => {
                CSharpReturnKind::WireDecodeObject {
                    class_name: id.into(),
                }
            }
            ReturnDef::Value(TypeExpr::Vec(inner)) => match inner.as_ref() {
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
            ReturnDef::Value(TypeExpr::Option(_)) => {
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
