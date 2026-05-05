use std::collections::HashSet;

use crate::ir::definitions::ReturnDef;
use crate::ir::ops::{ReadOp, ReadSeq};
use crate::ir::types::TypeExpr;

use super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpExpression, CSharpIdentity, CSharpLocalName,
    CSharpMethodName, CSharpType, CSharpTypeReference,
};
use super::super::plan::CSharpReturnKind;
use super::decode;
use super::lowerer::CSharpLowerer;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResultErrException {
    Generic { stringify: bool },
    Typed(CSharpClassName),
}

impl ResultErrException {
    fn class_name(&self) -> CSharpClassName {
        match self {
            Self::Generic { .. } => CSharpClassName::new("BoltException"),
            Self::Typed(class_name) => class_name.clone(),
        }
    }

    fn argument_expr(&self, err_decoded: CSharpExpression) -> CSharpExpression {
        match self {
            Self::Generic { stringify: true } => CSharpExpression::MethodCall {
                receiver: Box::new(err_decoded),
                method: CSharpMethodName::new("ToString"),
                type_args: vec![],
                args: CSharpArgumentList::default(),
            },
            Self::Generic { stringify: false } | Self::Typed(_) => err_decoded,
        }
    }
}

impl<'a> CSharpLowerer<'a> {
    /// Builds the [`CSharpReturnKind::WireDecodeResult`] for a
    /// `Result<Ok, Err>` return. This drives top-level functions and
    /// methods on classes, records, and enums.
    pub(super) fn result_return_kind(
        &self,
        return_def: &ReturnDef,
        decode_ops: Option<&ReadSeq>,
        shadowed: Option<&HashSet<CSharpClassName>>,
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
    /// expression. The [`ResultErrException`] classification keeps the
    /// exception class and constructor argument shape in one decision.
    fn result_throw_expr(
        &self,
        err_ty: &TypeExpr,
        err_decoded: CSharpExpression,
        shadowed: Option<&HashSet<CSharpClassName>>,
    ) -> CSharpExpression {
        let exception = self.result_err_exception(err_ty);
        let exception_type = CSharpType::Record(
            CSharpTypeReference::Plain(exception.class_name())
                .qualify_if_shadowed_opt(shadowed, &self.namespace),
        );
        let arg = exception.argument_expr(err_decoded);
        CSharpExpression::New {
            target: exception_type,
            args: vec![arg].into(),
        }
    }

    /// Classifies how an `Err` payload becomes a thrown C# exception:
    ///
    /// - `String`: generic `BoltException`, message-only.
    /// - `#[error]` enum or record: typed `<Name>Exception`, direct payload.
    /// - Anything else: generic `BoltException(value.ToString())`.
    fn result_err_exception(&self, err_ty: &TypeExpr) -> ResultErrException {
        match err_ty {
            TypeExpr::String => ResultErrException::Generic { stringify: false },
            TypeExpr::Enum(id)
                if self
                    .ffi
                    .catalog
                    .resolve_enum(id)
                    .is_some_and(|e| e.is_error) =>
            {
                let base: CSharpClassName = id.into();
                ResultErrException::Typed(CSharpClassName::exception_for(&base))
            }
            TypeExpr::Record(id)
                if self
                    .ffi
                    .catalog
                    .resolve_record(id)
                    .is_some_and(|r| r.is_error) =>
            {
                let base: CSharpClassName = id.into();
                ResultErrException::Typed(CSharpClassName::exception_for(&base))
            }
            _ => ResultErrException::Generic { stringify: true },
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
            lowerer.result_err_exception(&TypeExpr::String),
            ResultErrException::Generic { stringify: false },
        );
    }

    /// `#[error]` enum Err: the typed `<Name>Exception` carries the
    /// decoded enum value directly via its `Error` property. No
    /// stringification: the exception class binds the typed payload.
    #[test]
    fn result_err_path_for_error_enum_uses_typed_exception() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Enum(EnumId::new("error_enum"));
        assert_eq!(
            lowerer.result_err_exception(&err_ty),
            ResultErrException::Typed(CSharpClassName::new("ErrorEnumException")),
        );
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
            lowerer.result_err_exception(&err_ty),
            ResultErrException::Generic { stringify: true },
        );
    }

    /// `#[error]` record Err: same as the enum case: typed exception,
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
            lowerer.result_err_exception(&err_ty),
            ResultErrException::Typed(CSharpClassName::new("ErrorRecordException")),
        );
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
            lowerer.result_err_exception(&err_ty),
            ResultErrException::Generic { stringify: true },
        );
    }

    /// Primitive Err (`Result<i32, i32>`): the documented fallback
    /// path. The wrapper renders `BoltException(value.ToString())`.
    #[test]
    fn result_err_path_for_primitive_uses_bolt_exception_with_to_string() {
        let contract = contract_with_error_types();
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let options = CSharpOptions::default();
        let lowerer = CSharpLowerer::new(&contract, &abi, &options);

        let err_ty = TypeExpr::Primitive(PrimitiveType::I32);
        assert_eq!(
            lowerer.result_err_exception(&err_ty),
            ResultErrException::Generic { stringify: true },
        );
    }
}
