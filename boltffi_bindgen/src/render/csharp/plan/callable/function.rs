//! [`CSharpFunctionPlan`] (a top-level primitive function binding) and
//! [`CSharpReturnKind`] (how its return value crosses the ABI). The
//! binding serves the public wrapper method and the `[DllImport]`
//! native declaration at once; `CSharpReturnKind` decides whether the
//! native signature returns raw bytes (`FfiBuf`) or a CLR-marshalled
//! primitive.

use super::super::super::ast::{
    CSharpArgumentList, CSharpClassName, CSharpExpression, CSharpMethodName, CSharpParameterList,
    CSharpType,
};
use super::super::CFunctionName;
use super::{
    CSharpParamPlan, CSharpWireWriterPlan, native_call_arg_list, native_param_list,
    pinned_fixed_args,
};

/// A primitive function binding. Serves double duty: the template uses `name`
/// and C# types for the public static method, and `ffi_name` for the
/// `[DllImport]` entry point.
#[derive(Debug, Clone)]
pub struct CSharpFunctionPlan {
    /// Public wrapper method name.
    pub name: CSharpMethodName,
    /// Parameters with C# types.
    pub params: Vec<CSharpParamPlan>,
    /// C# return type as it appears in the public wrapper signature.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI. Drives how the wrapper body
    /// decodes the native return and what the `[DllImport]` signature looks
    /// like.
    pub return_kind: CSharpReturnKind,
    /// The C function this wrapper calls across the ABI boundary.
    pub ffi_name: CFunctionName,
    /// For each non-blittable record param, the setup code that wire-encodes
    /// it into a `byte[]` before the native call. Empty if the function has
    /// no wire-encoded params (blittable record params count as direct and
    /// do not appear here).
    pub wire_writers: Vec<CSharpWireWriterPlan>,
}

impl CSharpFunctionPlan {
    pub fn is_void(&self) -> bool {
        matches!(self.return_kind, CSharpReturnKind::Void)
    }

    /// Typed param list for the `[DllImport]` native signature.
    pub fn native_param_list(&self) -> CSharpParameterList {
        native_param_list(&self.params)
    }

    /// Typed argument list for the native invocation.
    pub fn native_call_args(&self) -> CSharpArgumentList {
        native_call_arg_list(&self.params)
    }

    /// Declarations for nested `fixed` statements pinning every
    /// [`CSharpParamKind::PinnedArray`](super::CSharpParamKind::PinnedArray) param in the signature.
    ///
    /// Rendered shape for a function with two pinned params:
    ///
    /// ```ignore
    /// [
    ///   "Location* _locationsPtr = locations",
    ///   "Trade* _tradesPtr = trades",
    /// ]
    /// ```
    ///
    /// The template wraps the call in `unsafe { fixed (...) { fixed (...)
    /// { ... } } }` so Rust reads directly from the C# heap without the
    /// GC relocating either managed array during the call.
    pub fn pinned_fixed_args(&self) -> Vec<String> {
        pinned_fixed_args(&self.params)
    }

    pub fn has_pinned_params(&self) -> bool {
        !self.pinned_fixed_args().is_empty()
    }
}

/// How a function's return value is delivered across the ABI. Drives
/// template branching on the wrapper-body shape; the templates own the
/// `return new WireReader(...)...` boilerplate around the variable bits
/// each variant carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpReturnKind {
    /// No return value.
    Void,
    /// Returned directly. Primitives, bools, and blittable records all
    /// share this path. The CLR already knows how to marshal them.
    Direct,
    /// `FfiBuf` carrying a wire-encoded `string`. Template owns the
    /// entire body.
    WireDecodeString,
    /// `FfiBuf` carrying a wire-encoded value with a static
    /// `Decode(WireReader)` method (non-blittable records, data enums).
    /// `class_name` is the receiver of that `Decode` call.
    WireDecodeObject { class_name: CSharpClassName },
    /// `FfiBuf` carrying a wire-encoded `Vec<T>` of a blittable
    /// primitive. Each primitive's wire shape picks a specific reader
    /// method: `bool` → `ReadBoolArray()`, `isize` → `ReadNIntArray()`,
    /// `usize` → `ReadNUIntArray()`, every other primitive →
    /// `ReadBlittableArray<T>()`. `type_arg` is `Some(T)` only for the
    /// generic `ReadBlittableArray<T>` form; the dedicated methods carry
    /// `None`.
    WireDecodeBlittablePrimitiveArray {
        method: CSharpMethodName,
        type_arg: Option<CSharpType>,
    },
    /// `FfiBuf` carrying a wire-encoded `Vec<T>` of a blittable record.
    /// Always renders as `ReadBlittableArray<{element}>()`.
    WireDecodeBlittableRecordArray { element: CSharpClassName },
    /// `FfiBuf` carrying a wire-encoded `Vec<T>` of a wire-encoded
    /// element (string, non-blittable record, nested vec, option).
    /// Renders as `ReadEncodedArray<{element_type}>({decode_lambda})`,
    /// where `decode_lambda` is the pre-rendered closure (`r0 => …`,
    /// `r1 => …`, depending on nesting depth — the lowerer's read
    /// counter assigns the closure parameter, so the template can't
    /// hardcode the name).
    WireDecodeEncodedArray {
        element_type: CSharpType,
        decode_lambda: CSharpExpression,
    },
    /// `FfiBuf` carrying a wire-encoded `Option<T>` (1-byte tag +
    /// optional payload). The inner decode walks the IR recursively
    /// across every inner shape (primitive, string, record, enum, vec,
    /// nested option), so `decode_expr` is the whole pre-rendered
    /// expression evaluated against a `reader` local the template
    /// introduces. Templates can't enumerate the recursion without
    /// recursive includes; this is the one decode shape where the
    /// expression genuinely belongs in the plan.
    WireDecodeOption { decode_expr: CSharpExpression },
}

impl CSharpReturnKind {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    /// Whether the native (DllImport) signature returns an `FfiBuf`.
    pub fn native_returns_ffi_buf(&self) -> bool {
        matches!(
            self,
            Self::WireDecodeString
                | Self::WireDecodeObject { .. }
                | Self::WireDecodeBlittablePrimitiveArray { .. }
                | Self::WireDecodeBlittableRecordArray { .. }
                | Self::WireDecodeEncodedArray { .. }
                | Self::WireDecodeOption { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::super::ast::{CSharpClassName, CSharpLocalName, CSharpParamName};
    use super::super::CSharpParamKind;
    use rstest::rstest;

    fn function_with_return(
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunctionPlan {
        CSharpFunctionPlan {
            name: CSharpMethodName::from_source("test"),
            params: vec![],
            return_type,
            return_kind,
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            wire_writers: vec![],
        }
    }

    fn param(name: &str, csharp_type: CSharpType, kind: CSharpParamKind) -> CSharpParamPlan {
        CSharpParamPlan {
            name: super::super::CSharpParamName::from_source(name),
            csharp_type,
            kind,
        }
    }

    fn record_type(name: &str) -> CSharpType {
        CSharpType::Record(CSharpClassName::from_source(name).into())
    }

    fn function_with_params(
        params: Vec<CSharpParamPlan>,
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunctionPlan {
        CSharpFunctionPlan {
            name: CSharpMethodName::from_source("test"),
            params,
            return_type,
            return_kind,
            ffi_name: CFunctionName::new("boltffi_test".to_string()),
            wire_writers: vec![],
        }
    }

    #[rstest]
    #[case::void(CSharpType::Void, CSharpReturnKind::Void, true)]
    #[case::int(CSharpType::Int, CSharpReturnKind::Direct, false)]
    #[case::bool(CSharpType::Bool, CSharpReturnKind::Direct, false)]
    #[case::double(CSharpType::Double, CSharpReturnKind::Direct, false)]
    fn is_void(
        #[case] return_type: CSharpType,
        #[case] return_kind: CSharpReturnKind,
        #[case] expected: bool,
    ) {
        assert_eq!(
            function_with_return(return_type, return_kind).is_void(),
            expected
        );
    }

    /// The native param list exposes each slot's marshalling shape: a
    /// string expands to a pair, bool gets a MarshalAs, and primitives
    /// stay bare. Mixed-shape case pins the spacing.
    #[test]
    fn native_param_list_expands_each_slot_by_kind() {
        let f = function_with_params(
            vec![
                param("flag", CSharpType::Bool, CSharpParamKind::Direct),
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
                param(
                    "person",
                    record_type("person"),
                    CSharpParamKind::WireEncoded {
                        binding_name: CSharpLocalName::for_bytes(&CSharpParamName::from_source(
                            "person",
                        )),
                    },
                ),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_param_list().to_string(),
            "[MarshalAs(UnmanagedType.I1)] bool flag, byte[] v, UIntPtr vLen, uint count, byte[] person, UIntPtr personLen",
        );
    }

    #[test]
    fn native_call_args_mirror_param_shapes() {
        let f = function_with_params(
            vec![
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_call_args().to_string(),
            "_vBytes, (UIntPtr)_vBytes.Length, count",
        );
    }
}
