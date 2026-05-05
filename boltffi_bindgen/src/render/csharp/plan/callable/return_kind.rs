//! Shared C# return-shape vocabulary for native calls.

use super::super::super::ast::{CSharpClassName, CSharpExpression, CSharpMethodName, CSharpType};

/// How a callable's return value is delivered across the ABI. Drives the
/// template's branching on the wrapper-body shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpReturnKind {
    /// No return value.
    Void,
    /// Returned directly. Primitives, bools, and blittable records share
    /// this path.
    Direct,
    /// `FfiBuf` carrying a wire-encoded `string`.
    WireDecodeString,
    /// `FfiBuf` carrying a wire-encoded value with a static
    /// `Decode(WireReader)` method (non-blittable records, data enums).
    /// `class_name` is the receiver of that `Decode` call.
    WireDecodeObject { class_name: CSharpClassName },
    /// `FfiBuf` carrying a wire-encoded `Vec<T>` of a blittable
    /// primitive. Each primitive's wire shape picks a specific reader
    /// method: `bool` -> `ReadBoolArray()`, `isize` -> `ReadNIntArray()`,
    /// `usize` -> `ReadNUIntArray()`, every other primitive ->
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
    /// where `decode_lambda` is a pre-rendered closure (e.g., `r0 => ...`)
    /// whose parameter name is assigned by the lowerer's read counter.
    WireDecodeEncodedArray {
        element_type: CSharpType,
        decode_lambda: CSharpExpression,
    },
    /// `FfiBuf` carrying a wire-encoded `Option<T>` (1-byte tag +
    /// optional payload). `decode_expr` is the pre-rendered decode
    /// expression, evaluated against a `reader` local the template
    /// introduces.
    WireDecodeOption { decode_expr: CSharpExpression },
    /// `FfiBuf` carrying a single wire-encoded value whose decode is
    /// not captured by the more specific shapes above. Used today for
    /// `Custom<Primitive>` returns: the macro side wraps the primitive
    /// in a wire buffer (so the C ABI is uniform across Custom types),
    /// and we reuse the same `var reader = ...; return ...;` shape as
    /// `WireDecodeOption` with a pre-rendered single-op decode.
    WireDecodeValue { decode_expr: CSharpExpression },
    /// `FfiBuf` carrying a wire-encoded `Result<T, E>`: a 1-byte tag
    /// (0 = Ok, non-zero = Err) followed by either the Ok payload or
    /// the Err payload. The wrapper body reads the tag, throws on Err,
    /// and otherwise returns the decoded Ok value. Both expressions
    /// are evaluated against a `reader` local the template introduces.
    ///
    /// `ok_decode_expr` is `None` when the Ok type is `Void`
    /// (`Result<(), E>`); the template emits no `return` for that case.
    WireDecodeResult {
        ok_decode_expr: Option<CSharpExpression>,
        err_throw_expr: CSharpExpression,
    },
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
                | Self::WireDecodeValue { .. }
                | Self::WireDecodeResult { .. }
        )
    }

    /// Whether the wrapper body throws on Err. Templates use this to
    /// pick the `if (reader.ReadU8() != 0) throw ...; return ...;`
    /// shape over the plain wire-decode shape.
    pub fn is_result(&self) -> bool {
        matches!(self, Self::WireDecodeResult { .. })
    }
}

pub(crate) fn native_return_type(
    return_type: &CSharpType,
    return_kind: &CSharpReturnKind,
) -> String {
    if return_kind.native_returns_ffi_buf() {
        "FfiBuf".to_string()
    } else {
        return_type.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::ast::{CSharpExpression, CSharpIdentity, CSharpLocalName};
    use super::*;

    fn dummy_throw_expr() -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("placeholder")))
    }

    /// `is_result` is true exactly for `WireDecodeResult`. Templates pivot on it
    /// to pick the throw-on-tag wrapper body over the plain decode body, so
    /// flipping any other arm to true would route a non-result return through
    /// the throwing template and emit `if (reader.ReadU8() != 0) throw ...`
    /// against bytes that have no error tag.
    #[test]
    fn is_result_is_true_only_for_wire_decode_result() {
        let result = CSharpReturnKind::WireDecodeResult {
            ok_decode_expr: None,
            err_throw_expr: dummy_throw_expr(),
        };
        assert!(result.is_result(), "WireDecodeResult is the throwing shape");

        for kind in [
            CSharpReturnKind::Void,
            CSharpReturnKind::Direct,
            CSharpReturnKind::WireDecodeString,
            CSharpReturnKind::WireDecodeOption {
                decode_expr: dummy_throw_expr(),
            },
        ] {
            assert!(
                !kind.is_result(),
                "non-result return kind {kind:?} must not opt into the throwing template",
            );
        }
    }

    /// `Result<_, _>` returns travel as `FfiBuf` on the wire, so they must
    /// satisfy `native_returns_ffi_buf` alongside the other wire-decoded
    /// shapes; otherwise the DllImport signature would be generated as a
    /// primitive return.
    #[test]
    fn wire_decode_result_native_returns_ffi_buf() {
        let kind = CSharpReturnKind::WireDecodeResult {
            ok_decode_expr: None,
            err_throw_expr: dummy_throw_expr(),
        };
        assert!(kind.native_returns_ffi_buf());
    }
}
