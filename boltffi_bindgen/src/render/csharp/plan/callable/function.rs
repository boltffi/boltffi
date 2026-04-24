//! [`CSharpFunction`] (a top-level primitive function binding) and
//! [`CSharpReturnKind`] (how its return value crosses the ABI). The
//! binding serves the public wrapper method and the `[DllImport]`
//! native declaration at once; `CSharpReturnKind` decides whether the
//! native signature returns raw bytes (`FfiBuf`) or a CLR-marshalled
//! primitive.

use super::super::CSharpType;
use super::{CSharpParam, CSharpWireWriter, pinned_fixed_args};

/// A primitive function binding. Serves double duty: the template uses `name`
/// and C# types for the public static method, and `ffi_name` for the
/// `[DllImport]` entry point.
#[derive(Debug, Clone)]
pub struct CSharpFunction {
    /// PascalCase method name (e.g., `"EchoI32"`).
    pub name: String,
    /// Parameters with C# types.
    pub params: Vec<CSharpParam>,
    /// C# return type as it appears in the public wrapper signature.
    pub return_type: CSharpType,
    /// How the return value crosses the ABI. Drives how the wrapper body
    /// decodes the native return and what the `[DllImport]` signature looks
    /// like.
    pub return_kind: CSharpReturnKind,
    /// The C symbol name (e.g., `"boltffi_echo_i32"`).
    pub ffi_name: String,
    /// For each non-blittable record param, the setup code that wire-encodes
    /// it into a `byte[]` before the native call. Empty if the function has
    /// no wire-encoded params (blittable record params count as direct and
    /// do not appear here).
    pub wire_writers: Vec<CSharpWireWriter>,
}

impl CSharpFunction {
    pub fn is_void(&self) -> bool {
        matches!(self.return_kind, CSharpReturnKind::Void)
    }

    /// Comma-joined param declarations as they appear in the public
    /// wrapper signature.
    pub fn wrapper_param_list(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::wrapper_declaration)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-joined param declarations as they appear in the
    /// `[DllImport]` native signature.
    pub fn native_param_list(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::native_declaration)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Comma-joined call arguments handed to the native invocation.
    pub fn native_call_args(&self) -> String {
        self.params
            .iter()
            .map(CSharpParam::native_call_arg)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The return type used in the `[DllImport]` signature. Wire-encoded
    /// returns come back as an `FfiBuf`; everything else (primitives,
    /// bools, blittable records) uses the C# type directly.
    pub fn native_return_type(&self) -> String {
        if self.return_kind.native_returns_ffi_buf() {
            "FfiBuf".to_string()
        } else {
            self.return_type.to_string()
        }
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

/// How a function's return value is delivered across the ABI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSharpReturnKind {
    /// No return value.
    Void,
    /// Returned directly. Primitives, bools, and blittable records all
    /// share this path. The CLR already knows how to marshal them.
    Direct,
    /// The native function returns an `FfiBuf`. The wrapper copies the
    /// bytes into a managed `string` via `WireReader.ReadString` and
    /// frees the buffer.
    WireDecodeString,
    /// The native function returns an `FfiBuf` carrying a wire-encoded
    /// value with a static `Decode(WireReader)` method. The wrapper wraps
    /// it in a `WireReader` and calls `{class_name}.Decode(reader)` to
    /// reconstruct the value. Used for non-blittable records and data
    /// enums, whose rendered C# types both expose the same `Decode` API
    /// at the call site.
    WireDecodeObject { class_name: String },
    /// The native function returns an `FfiBuf` carrying a wire-encoded
    /// `Vec<T>`. The wrapper wraps it in a `WireReader` and invokes
    /// `reader_call` on the reader to reconstruct the managed `T[]`.
    /// `reader_call` is the full method invocation without the receiver,
    /// e.g. `ReadBlittableArray<int>()` for `Vec<i32>` or
    /// `ReadBoolArray()` for `Vec<bool>`.
    WireDecodeArray { reader_call: String },
    /// The native function returns an `FfiBuf` carrying a wire-encoded
    /// `Option<T>` (1-byte tag + optional payload). The wrapper wraps
    /// it in a `WireReader` named `reader` and evaluates `decode_expr`,
    /// which emit has already rendered against that reader so it
    /// handles every inner shape (primitive, string, record, enum, vec)
    /// without per-shape branching here.
    WireDecodeOption { decode_expr: String },
}

impl CSharpReturnKind {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    pub fn is_wire_decode_string(&self) -> bool {
        matches!(self, Self::WireDecodeString)
    }

    pub fn is_wire_decode_object(&self) -> bool {
        matches!(self, Self::WireDecodeObject { .. })
    }

    pub fn is_wire_decode_array(&self) -> bool {
        matches!(self, Self::WireDecodeArray { .. })
    }

    pub fn is_wire_decode_option(&self) -> bool {
        matches!(self, Self::WireDecodeOption { .. })
    }

    /// Whether the native (DllImport) signature returns an `FfiBuf`.
    pub fn native_returns_ffi_buf(&self) -> bool {
        matches!(
            self,
            Self::WireDecodeString
                | Self::WireDecodeObject { .. }
                | Self::WireDecodeArray { .. }
                | Self::WireDecodeOption { .. }
        )
    }

    /// For `WireDecodeObject`, the decoded C# class name (e.g., `"Point"`
    /// for a record, `"Shape"` for a data enum); `None` for every other
    /// kind. Templates use this to emit `{class_name}.Decode`.
    pub fn decode_class_name(&self) -> Option<&str> {
        match self {
            Self::WireDecodeObject { class_name } => Some(class_name),
            _ => None,
        }
    }

    /// The `return` statement that goes inside the `try` block of a
    /// wire-decoded call body. `buf_var` is the local name holding the
    /// `FfiBuf` from the native call. Returns `None` for non-wire-decoded
    /// kinds so callers cannot misuse an empty-string fallback as valid
    /// generated code.
    pub fn wire_decode_return(&self, buf_var: &str) -> Option<String> {
        match self {
            Self::WireDecodeString => {
                Some(format!("return new WireReader({}).ReadString();", buf_var))
            }
            Self::WireDecodeObject { class_name } => Some(format!(
                "return {}.Decode(new WireReader({}));",
                class_name, buf_var
            )),
            Self::WireDecodeArray { reader_call } => Some(format!(
                "return new WireReader({}).{};",
                buf_var, reader_call
            )),
            Self::WireDecodeOption { decode_expr } => Some(format!(
                "var reader = new WireReader({}); return {};",
                buf_var, decode_expr
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::CSharpParamKind;
    use rstest::rstest;

    fn function_with_return(
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunction {
        CSharpFunction {
            name: "Test".to_string(),
            params: vec![],
            return_type,
            return_kind,
            ffi_name: "boltffi_test".to_string(),
            wire_writers: vec![],
        }
    }

    fn param(name: &str, csharp_type: CSharpType, kind: CSharpParamKind) -> CSharpParam {
        CSharpParam {
            name: name.to_string(),
            csharp_type,
            kind,
        }
    }

    fn function_with_params(
        params: Vec<CSharpParam>,
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpFunction {
        CSharpFunction {
            name: "Test".to_string(),
            params,
            return_type,
            return_kind,
            ffi_name: "boltffi_test".to_string(),
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

    #[test]
    fn wrapper_param_list_joins_with_comma_space() {
        let f = function_with_params(
            vec![
                param("a", CSharpType::Int, CSharpParamKind::Direct),
                param("b", CSharpType::String, CSharpParamKind::Utf8Bytes),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(f.wrapper_param_list(), "int a, string b");
    }

    #[test]
    fn wrapper_param_list_empty_for_no_params() {
        let f = function_with_params(vec![], CSharpType::Void, CSharpReturnKind::Void);
        assert_eq!(f.wrapper_param_list(), "");
    }

    /// The native param list exposes each slot's marshalling shape: a
    /// string expands to a pair, bool gets a MarshalAs, and primitives
    /// stay bare. This is the one place the different shapes must line
    /// up, so we pin it with a mixed-shape case.
    #[test]
    fn native_param_list_expands_each_slot_by_kind() {
        let f = function_with_params(
            vec![
                param("flag", CSharpType::Bool, CSharpParamKind::Direct),
                param("v", CSharpType::String, CSharpParamKind::Utf8Bytes),
                param("count", CSharpType::UInt, CSharpParamKind::Direct),
                param(
                    "person",
                    CSharpType::Record("Person".to_string()),
                    CSharpParamKind::WireEncoded {
                        binding_name: "_personBytes".to_string(),
                    },
                ),
            ],
            CSharpType::Void,
            CSharpReturnKind::Void,
        );
        assert_eq!(
            f.native_param_list(),
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
            f.native_call_args(),
            "_vBytes, (UIntPtr)_vBytes.Length, count",
        );
    }

    /// Wire-encoded returns (string, non-blittable record) come back as
    /// an `FfiBuf` in the native signature regardless of the wrapper's
    /// public return type.
    #[rstest]
    #[case::void(CSharpType::Void, CSharpReturnKind::Void, "void")]
    #[case::primitive(CSharpType::Int, CSharpReturnKind::Direct, "int")]
    #[case::blittable_record(
        CSharpType::Record("Point".to_string()),
        CSharpReturnKind::Direct,
        "Point",
    )]
    #[case::string(CSharpType::String, CSharpReturnKind::WireDecodeString, "FfiBuf")]
    #[case::wire_record(
        CSharpType::Record("Person".to_string()),
        CSharpReturnKind::WireDecodeObject { class_name: "Person".to_string() },
        "FfiBuf",
    )]
    #[case::option_primitive(
        CSharpType::Nullable(Box::new(CSharpType::Int)),
        CSharpReturnKind::WireDecodeOption {
            decode_expr: "reader.ReadU8() == 0 ? (int?)null : reader.ReadI32()".to_string(),
        },
        "FfiBuf",
    )]
    fn native_return_type_reflects_ffi_buf_paths(
        #[case] return_type: CSharpType,
        #[case] return_kind: CSharpReturnKind,
        #[case] expected: &str,
    ) {
        assert_eq!(
            function_with_return(return_type, return_kind).native_return_type(),
            expected
        );
    }

    #[test]
    fn wire_decode_return_for_string_uses_read_string() {
        let kind = CSharpReturnKind::WireDecodeString;
        assert_eq!(
            kind.wire_decode_return("_buf").as_deref(),
            Some("return new WireReader(_buf).ReadString();"),
        );
    }

    #[test]
    fn wire_decode_return_for_object_calls_decode() {
        let kind = CSharpReturnKind::WireDecodeObject {
            class_name: "Person".to_string(),
        };
        assert_eq!(
            kind.wire_decode_return("_buf").as_deref(),
            Some("return Person.Decode(new WireReader(_buf));"),
        );
    }

    /// `WireDecodeOption` wraps the pre-rendered `decode_expr` in a local
    /// `reader` binding so the emit-time decoder can reference `reader`
    /// multiple times (once for the 1-byte tag, again for the payload)
    /// without duplicating buffer construction.
    #[test]
    fn wire_decode_return_for_option_binds_reader_local() {
        let kind = CSharpReturnKind::WireDecodeOption {
            decode_expr: "reader.ReadU8() == 0 ? (int?)null : reader.ReadI32()".to_string(),
        };
        assert_eq!(
            kind.wire_decode_return("_buf").as_deref(),
            Some(
                "var reader = new WireReader(_buf); return reader.ReadU8() == 0 ? (int?)null : reader.ReadI32();"
            ),
        );
    }

    #[rstest]
    #[case::void(CSharpReturnKind::Void)]
    #[case::direct(CSharpReturnKind::Direct)]
    fn wire_decode_return_none_for_non_wire_kinds(#[case] kind: CSharpReturnKind) {
        assert_eq!(kind.wire_decode_return("_buf"), None);
    }

    #[test]
    fn decode_class_name_some_only_for_wire_decode_object() {
        assert_eq!(
            CSharpReturnKind::WireDecodeObject {
                class_name: "Point".to_string()
            }
            .decode_class_name(),
            Some("Point"),
        );
        assert_eq!(CSharpReturnKind::WireDecodeString.decode_class_name(), None);
        assert_eq!(CSharpReturnKind::Void.decode_class_name(), None);
        assert_eq!(CSharpReturnKind::Direct.decode_class_name(), None);
    }
}
