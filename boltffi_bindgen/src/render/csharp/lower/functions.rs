use std::collections::HashSet;

use boltffi_ffi_rules::naming;

use crate::ir::definitions::{FunctionDef, ReturnDef};
use crate::ir::ops::{ReadOp, ReadSeq};
use crate::ir::types::TypeExpr;

use super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpComment, CSharpExpression, CSharpIdentity,
    CSharpLocalName, CSharpType, CSharpTypeReference,
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
            summary_doc: CSharpComment::from_str_option(function.doc.as_deref()),
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

impl<'a> CSharpLowerer<'a> {
    /// Builds the [`CSharpReturnKind::WireDecodeResult`] for a
    /// `Result<Ok, Err>` return: pre-renders the Ok decode against a
    /// `reader` local, plus the throw expression that constructs the
    /// generated exception type from the wire-decoded Err payload.
    pub(super) fn result_return_kind(
        &self,
        return_def: &ReturnDef,
        decode_ops: Option<&ReadSeq>,
        shadowed: Option<&std::collections::HashSet<CSharpClassName>>,
    ) -> CSharpReturnKind {
        let (ok_ty, err_ty) = match return_def {
            ReturnDef::Result { ok, err } => (ok, err),
            other => panic!("result_return_kind called with non-result return: {other:?}"),
        };
        let result_seq = decode_ops.expect("Result return must carry decode_ops");
        let (ok_seq, err_seq) = match result_seq.ops.first() {
            Some(ReadOp::Result { ok, err, .. }) => (ok.as_ref(), err.as_ref()),
            other => panic!("expected ReadOp::Result, got {other:?}"),
        };

        let reader =
            CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("reader")));
        let mut locals = decode::DecodeLocalCounters::default();

        let ok_decode_expr = if matches!(ok_ty, TypeExpr::Void) {
            None
        } else {
            Some(decode::lower_decode_expr(
                ok_seq,
                &reader,
                shadowed,
                &self.namespace,
                &mut locals,
            ))
        };

        let err_decoded =
            decode::lower_decode_expr(err_seq, &reader, shadowed, &self.namespace, &mut locals);
        let err_throw_expr = self.result_throw_expr(err_ty, err_decoded, shadowed);

        CSharpReturnKind::WireDecodeResult {
            ok_decode_expr,
            err_throw_expr,
        }
    }

    /// Wraps the wire-decoded Err payload in a `new <Exception>(...)`
    /// expression. The exception class depends on the Err type:
    ///
    /// - `String`: the runtime `BoltException`, message-only.
    /// - `#[error]` enum or record: the generated `<Type>Exception`
    ///   wrapper that holds the typed value.
    /// - Anything else (a non-error type used as `Err`): wrap in
    ///   `BoltException(value.ToString())` so the call site still has
    ///   something to catch. Lowerer admission already gates on
    ///   `is_supported_result_type`, so this fallback only matters for
    ///   non-error primitives or records used as errors.
    fn result_throw_expr(
        &self,
        err_ty: &TypeExpr,
        err_decoded: CSharpExpression,
        shadowed: Option<&std::collections::HashSet<CSharpClassName>>,
    ) -> CSharpExpression {
        let exception_class = self.result_err_exception_class(err_ty);
        let exception_type = CSharpType::Record(
            CSharpTypeReference::Plain(exception_class.clone())
                .qualify_if_shadowed_opt(shadowed, &self.namespace),
        );
        let arg = if self.result_err_needs_to_string(err_ty) {
            CSharpExpression::MethodCall {
                receiver: Box::new(err_decoded),
                method: super::super::ast::CSharpMethodName::new("ToString"),
                type_args: vec![],
                args: CSharpArgumentList::default(),
            }
        } else {
            err_decoded
        };
        CSharpExpression::New {
            target: exception_type,
            args: vec![arg].into(),
        }
    }

    /// Picks the C# exception class for an `Err` type. Strings and
    /// non-error types funnel through the runtime `BoltException`;
    /// `#[error]` enums and records get a typed `<Name>Exception` so
    /// the caller can `catch` them specifically and read the original
    /// value off the `Error` property.
    fn result_err_exception_class(&self, err_ty: &TypeExpr) -> CSharpClassName {
        match err_ty {
            TypeExpr::Enum(id)
                if self
                    .ffi
                    .catalog
                    .resolve_enum(id)
                    .is_some_and(|e| e.is_error) =>
            {
                let base: CSharpClassName = id.into();
                CSharpClassName::exception_for(&base)
            }
            TypeExpr::Record(id)
                if self
                    .ffi
                    .catalog
                    .resolve_record(id)
                    .is_some_and(|r| r.is_error) =>
            {
                let base: CSharpClassName = id.into();
                CSharpClassName::exception_for(&base)
            }
            _ => CSharpClassName::new("BoltException"),
        }
    }

    /// `BoltException` always takes a `string` message, so non-string
    /// Err payloads going through the generic path need `.ToString()`.
    /// Typed `<Name>Exception` constructors take the error value
    /// directly and so never need to stringify it.
    fn result_err_needs_to_string(&self, err_ty: &TypeExpr) -> bool {
        match err_ty {
            TypeExpr::String => false,
            TypeExpr::Enum(id) => !self
                .ffi
                .catalog
                .resolve_enum(id)
                .is_some_and(|e| e.is_error),
            TypeExpr::Record(id) => !self
                .ffi
                .catalog
                .resolve_record(id)
                .is_some_and(|r| r.is_error),
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::FfiContract;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::PackageInfo;
    use crate::ir::definitions::{CStyleVariant, EnumDef, EnumRepr, FieldDef, RecordDef};
    use crate::ir::ids::{EnumId, RecordId};
    use crate::ir::types::PrimitiveType;

    use super::super::super::CSharpOptions;

    fn enum_with_error_flag(id: &str, is_error: bool) -> EnumDef {
        EnumDef {
            id: EnumId::new(id),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![CStyleVariant {
                    name: "Variant".into(),
                    discriminant: 0,
                    doc: None,
                }],
            },
            is_error,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn record_with_error_flag(id: &str, is_error: bool) -> RecordDef {
        RecordDef {
            id: RecordId::new(id),
            is_repr_c: false,
            is_error,
            fields: vec![FieldDef {
                name: "Code".into(),
                type_expr: TypeExpr::Primitive(PrimitiveType::I32),
                doc: None,
                default: None,
            }],
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn contract_with_error_types() -> FfiContract {
        let mut contract = FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        };
        contract
            .catalog
            .insert_enum(enum_with_error_flag("error_enum", true));
        contract
            .catalog
            .insert_enum(enum_with_error_flag("plain_enum", false));
        contract
            .catalog
            .insert_record(record_with_error_flag("error_record", true));
        contract
            .catalog
            .insert_record(record_with_error_flag("plain_record", false));
        contract
    }

    /// String Err: the runtime `BoltException` carries the message
    /// verbatim. The constructor takes `string`, so the throw
    /// expression doesn't need a `.ToString()` wrap.
    #[test]
    fn result_err_path_for_string_uses_bolt_exception_without_to_string() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        assert_eq!(
            lowerer
                .result_err_exception_class(&TypeExpr::String)
                .as_str(),
            "BoltException",
        );
        assert!(!lowerer.result_err_needs_to_string(&TypeExpr::String));
    }

    /// `#[error]` enum Err: the typed `<Name>Exception` carries the
    /// decoded enum value directly via its `Error` property. No
    /// stringification — the exception class binds the typed payload.
    #[test]
    fn result_err_path_for_error_enum_uses_typed_exception() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Enum(EnumId::new("error_enum"));
        assert_eq!(
            lowerer.result_err_exception_class(&err_ty).as_str(),
            "ErrorEnumException",
        );
        assert!(!lowerer.result_err_needs_to_string(&err_ty));
    }

    /// Non-error enum Err: falls back to `BoltException(value.ToString())`.
    /// Pinning this catches a regression where the `is_error` predicate
    /// drifts to "any enum qualifies" and silently routes plain enums
    /// to undeclared `<Name>Exception` classes.
    #[test]
    fn result_err_path_for_non_error_enum_falls_back_to_bolt_exception() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Enum(EnumId::new("plain_enum"));
        assert_eq!(
            lowerer.result_err_exception_class(&err_ty).as_str(),
            "BoltException",
        );
        assert!(lowerer.result_err_needs_to_string(&err_ty));
    }

    /// `#[error]` record Err: same as the enum case — typed exception,
    /// no `.ToString()`. The enum and record paths share their wrapper
    /// shape; pinning both catches any divergence.
    #[test]
    fn result_err_path_for_error_record_uses_typed_exception() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Record(RecordId::new("error_record"));
        assert_eq!(
            lowerer.result_err_exception_class(&err_ty).as_str(),
            "ErrorRecordException",
        );
        assert!(!lowerer.result_err_needs_to_string(&err_ty));
    }

    /// Non-error record Err: falls back to BoltException with `.ToString()`.
    /// Pin the negative case so the predicate stays anchored on
    /// `is_error` rather than drifting toward "any record qualifies".
    #[test]
    fn result_err_path_for_non_error_record_falls_back_to_bolt_exception() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Record(RecordId::new("plain_record"));
        assert_eq!(
            lowerer.result_err_exception_class(&err_ty).as_str(),
            "BoltException",
        );
        assert!(lowerer.result_err_needs_to_string(&err_ty));
    }

    /// Primitive Err (`Result<i32, i32>`): the documented fallback
    /// path. The wrapper renders `BoltException(value.ToString())`.
    /// The demo's `result_with_int_error` exercises this end-to-end;
    /// this test pins the predicate inputs directly so the unit gate
    /// catches a regression even without the demo binary.
    #[test]
    fn result_err_path_for_primitive_uses_bolt_exception_with_to_string() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Primitive(PrimitiveType::I32);
        assert_eq!(
            lowerer.result_err_exception_class(&err_ty).as_str(),
            "BoltException",
        );
        assert!(lowerer.result_err_needs_to_string(&err_ty));
    }
}
