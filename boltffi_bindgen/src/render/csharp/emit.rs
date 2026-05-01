//! Orchestrator: [`CSharpEmitter::emit`] runs the lowerer, then renders
//! each plan entry through its Askama template, assembling the
//! [`CSharpOutput`]. All "ABI op → C# syntax" translation lives in
//! [`super::lower`](super::lower) (the size, encode, and decode
//! sub-modules); this module contains no syntax helpers of its own.

use askama::Template as _;

use crate::ir::{AbiContract, FfiContract};

use super::{
    CSharpOptions,
    lower::CSharpLowerer,
    plan::CSharpEnumKind,
    templates::{
        ClassTemplate, EnumCStyleTemplate, EnumDataTemplate, FunctionsTemplate, NativeTemplate,
        PreambleTemplate, RecordTemplate,
    },
};

/// A single generated `.cs` file: its file name (relative to the output
/// directory) and its full source text.
#[derive(Debug, Clone)]
pub struct CSharpFile {
    pub file_name: String,
    pub source: String,
}

/// The rendered C# output: one file per record plus a main file with the
/// wrapper class and `[DllImport]` declarations.
#[derive(Debug, Clone)]
pub struct CSharpOutput {
    pub files: Vec<CSharpFile>,
}

impl CSharpOutput {
    /// Concatenation of every file's source text. Convenience for tests
    /// and spot-checks that only care about "did this snippet appear
    /// anywhere in the generated code?"
    #[cfg(test)]
    pub(crate) fn combined_source(&self) -> String {
        self.files
            .iter()
            .map(|f| f.source.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Entry point for C# code generation.
pub struct CSharpEmitter;

impl CSharpEmitter {
    pub fn emit(ffi: &FfiContract, abi: &AbiContract, options: &CSharpOptions) -> CSharpOutput {
        let lowerer = CSharpLowerer::new(ffi, abi, options);
        let module = lowerer.lower();

        let mut files: Vec<CSharpFile> = module
            .records
            .iter()
            .map(|record| CSharpFile {
                file_name: format!("{}.cs", record.class_name),
                source: RecordTemplate {
                    record,
                    namespace: &module.namespace,
                }
                .render()
                .unwrap_or_else(|err| panic!("record {} render failed: {err}", record.class_name)),
            })
            .collect();

        files.extend(module.enums.iter().map(|enumeration| {
            CSharpFile {
                file_name: format!("{}.cs", enumeration.class_name),
                source: match enumeration.kind {
                    CSharpEnumKind::CStyle => EnumCStyleTemplate {
                        enumeration,
                        namespace: &module.namespace,
                    }
                    .render()
                    .unwrap_or_else(|err| {
                        panic!(
                            "c-style enum {} render failed: {err}",
                            enumeration.class_name
                        )
                    }),
                    CSharpEnumKind::Data => EnumDataTemplate {
                        enumeration,
                        namespace: &module.namespace,
                    }
                    .render()
                    .unwrap_or_else(|err| {
                        panic!("data enum {} render failed: {err}", enumeration.class_name)
                    }),
                },
            }
        }));

        files.extend(module.classes.iter().map(|class| {
            CSharpFile {
                file_name: format!("{}.cs", class.class_name),
                source: ClassTemplate {
                    class,
                    namespace: &module.namespace,
                }
                .render()
                .unwrap_or_else(|err| panic!("class {} render failed: {err}", class.class_name)),
            }
        }));

        let mut main_source = String::new();
        main_source.push_str(&PreambleTemplate { module: &module }.render().unwrap());
        main_source.push('\n');
        main_source.push_str(&FunctionsTemplate { module: &module }.render().unwrap());
        main_source.push_str(&NativeTemplate { module: &module }.render().unwrap());
        main_source.push('\n');

        files.push(CSharpFile {
            file_name: format!("{}.cs", module.class_name),
            source: main_source,
        });

        CSharpOutput { files }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CStyleVariant, ClassDef, DataVariant, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef,
        ParamDef, ParamPassing, Receiver, RecordDef, ReturnDef, VariantPayload,
    };
    use crate::ir::ids::{ClassId, EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId};
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use boltffi_ffi_rules::callable::ExecutionKind;

    fn empty_contract() -> FfiContract {
        FfiContract {
            package: PackageInfo {
                name: "demo_lib".to_string(),
                version: None,
            },
            functions: vec![],
            catalog: Default::default(),
        }
    }

    fn primitive_function(
        name: &str,
        params: Vec<(&str, PrimitiveType)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(name),
            params: params
                .into_iter()
                .map(|(param_name, prim)| ParamDef {
                    name: ParamName::new(param_name),
                    type_expr: TypeExpr::Primitive(prim),
                    passing: ParamPassing::Value,
                    doc: None,
                })
                .collect(),
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    fn emit_contract(contract: &FfiContract) -> CSharpOutput {
        let abi = IrLowerer::new(contract).to_abi_contract();
        CSharpEmitter::emit(contract, &abi, &CSharpOptions::default())
    }

    fn assert_source_contains(source: &str, snippet: &str, expecting: &str) {
        assert!(
            source.contains(snippet),
            "expecting {expecting}\n  missing snippet: {snippet:?}"
        );
    }

    fn assert_source_lacks(source: &str, snippet: &str, expecting: &str) {
        assert!(
            !source.contains(snippet),
            "expecting {expecting}\n  unexpected snippet: {snippet:?}"
        );
    }

    #[test]
    fn emit_primitive_function_generates_wrapper_and_native() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "echo_i32",
            vec![("value", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let src = emit_contract(&contract).combined_source();

        assert!(src.contains("public static int EchoI32(int value)"));
        assert!(src.contains("return NativeMethods.EchoI32(value);"));
        assert!(src.contains(r#"[DllImport(LibName, EntryPoint = "boltffi_echo_i32")]"#));
        assert!(src.contains("internal static extern int EchoI32(int value);"));
    }

    #[test]
    fn emit_void_function_omits_return_keyword() {
        let mut contract = empty_contract();
        contract
            .functions
            .push(primitive_function("noop", vec![], ReturnDef::Void));

        let src = emit_contract(&contract).combined_source();

        assert!(src.contains("public static void Noop()"));
        assert!(src.contains("NativeMethods.Noop();"));
        assert!(!src.contains("return NativeMethods.Noop()"));
    }

    #[test]
    fn emit_unsigned_types_use_csharp_unsigned_keywords() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "unsigned_echo",
            vec![
                ("a", PrimitiveType::U8),
                ("b", PrimitiveType::U16),
                ("c", PrimitiveType::U32),
                ("d", PrimitiveType::U64),
            ],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
        ));

        let src = emit_contract(&contract).combined_source();

        assert!(src.contains("uint UnsignedEcho(byte a, ushort b, uint c, ulong d)"));
    }

    #[test]
    fn emit_namespace_and_class_use_pascal_case() {
        let contract = empty_contract();
        let output = emit_contract(&contract);

        assert!(output.combined_source().contains("namespace DemoLib"));
    }

    #[test]
    fn emit_escapes_csharp_keywords_in_param_names() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "test_keywords",
            vec![("int", PrimitiveType::I32), ("value", PrimitiveType::I32)],
            ReturnDef::Void,
        ));

        let src = emit_contract(&contract).combined_source();

        assert!(src.contains("@int"));
    }

    fn function_with_types(
        name: &str,
        params: Vec<(&str, TypeExpr)>,
        returns: ReturnDef,
    ) -> FunctionDef {
        FunctionDef {
            id: FunctionId::new(name),
            params: params
                .into_iter()
                .map(|(param_name, type_expr)| ParamDef {
                    name: ParamName::new(param_name),
                    type_expr,
                    passing: ParamPassing::Value,
                    doc: None,
                })
                .collect(),
            returns,
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        }
    }

    /// C# P/Invoke marshals `bool` as a 4-byte Win32 BOOL by default, but
    /// BoltFFI's C ABI uses a 1-byte native bool, so the generated native
    /// signature must force `UnmanagedType.I1` for both param and return.
    #[test]
    fn emit_bool_function_uses_i1_marshalling_for_native_signature() {
        let mut contract = empty_contract();
        contract.functions.push(primitive_function(
            "flip",
            vec![("value", PrimitiveType::Bool)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::Bool)),
        ));

        let src = emit_contract(&contract).combined_source();

        assert!(src.contains("public static bool Flip(bool value)"));
        assert!(src.contains("[return: MarshalAs(UnmanagedType.I1)]"));
        assert!(src.contains(
            "internal static extern bool Flip([MarshalAs(UnmanagedType.I1)] bool value);"
        ));
    }

    /// The C ABI for a `String` parameter is `(const uint8_t* ptr, uintptr_t len)`,
    /// which on the C# side becomes a `byte[]` + `UIntPtr` pair. The wrapper
    /// exposes a plain `string` and UTF-8 encodes it just before the native call.
    #[test]
    fn emit_string_param_marshals_as_byte_array_and_length() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "string_length",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::U32)),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static uint StringLength(string v)",
            "the public wrapper to expose the string param as a plain C# `string`",
        );
        assert_source_contains(
            &src,
            "byte[] _vBytes = Encoding.UTF8.GetBytes(v);",
            "UTF-8 encoding of the string into a managed byte[] before the P/Invoke call",
        );
        assert_source_contains(
            &src,
            "NativeMethods.StringLength(_vBytes, (UIntPtr)_vBytes.Length)",
            "the native call to receive the encoded byte[] and its length as two separate arguments",
        );
        assert_source_contains(
            &src,
            "internal static extern uint StringLength(byte[] v, UIntPtr vLen);",
            "the P/Invoke declaration to split a string param into (byte[], UIntPtr) matching the C ABI",
        );
    }

    /// A `String` return is wire-encoded by Rust into a length-prefixed
    /// `FfiBuf` (i32 length + UTF-8 bytes). The wrapper decodes via
    /// `WireReader.ReadString` and must release the native allocation with
    /// `FreeBuf` even if decoding throws.
    #[test]
    fn emit_string_return_decodes_ffibuf_and_frees() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_string",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::String),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static string EchoString(string v)",
            "the public wrapper to hide the FfiBuf and expose a normal `string` return",
        );
        assert_source_contains(
            &src,
            "FfiBuf _buf = NativeMethods.EchoString(",
            "the native return captured in an `FfiBuf _buf` local so it can be decoded and freed",
        );
        assert_source_contains(
            &src,
            "new WireReader(_buf).ReadString()",
            "WireReader stateful decode of the FfiBuf-carried string, shared with the record decode path",
        );
        assert_source_contains(
            &src,
            "NativeMethods.FreeBuf(_buf);",
            "a FreeBuf call in a finally block so the Rust allocator reclaims the buffer even if decoding throws",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoString(byte[] v, UIntPtr vLen);",
            "the P/Invoke signature to return FfiBuf rather than a bare string",
        );
    }

    /// The `FfiBuf` struct, `WireReader`, and `FreeBuf` DllImport are only
    /// needed when a module actually traffics in wire-encoded returns.
    /// Primitive-only output should not carry the extra helpers.
    #[test]
    fn emit_string_helpers_only_appear_when_strings_are_used() {
        let mut primitive_only = empty_contract();
        primitive_only.functions.push(primitive_function(
            "add",
            vec![("a", PrimitiveType::I32), ("b", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));
        let primitive_src = emit_contract(&primitive_only).combined_source();

        assert_source_lacks(
            &primitive_src,
            "FfiBuf",
            "no FfiBuf struct or references in primitive-only output",
        );
        assert_source_lacks(
            &primitive_src,
            "WireReader",
            "no WireReader helper class in primitive-only output",
        );
        assert_source_lacks(
            &primitive_src,
            "FreeBuf",
            "no FreeBuf DllImport in primitive-only output",
        );
        assert_source_lacks(
            &primitive_src,
            "using System.Text;",
            "no System.Text using directive when Encoding.UTF8 is never referenced",
        );

        let mut with_string = empty_contract();
        with_string.functions.push(function_with_types(
            "echo",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::String),
        ));
        let string_src = emit_contract(&with_string).combined_source();

        assert_source_contains(
            &string_src,
            "internal struct FfiBuf",
            "the FfiBuf struct when strings are used (mirrors the Rust FfiBuf_u8 layout)",
        );
        assert_source_contains(
            &string_src,
            "WireReader",
            "a WireReader helper when strings are used",
        );
        assert_source_contains(
            &string_src,
            r#"[DllImport(LibName, EntryPoint = "boltffi_free_buf")]"#,
            "a DllImport binding to boltffi_free_buf when strings are used",
        );
        assert_source_contains(
            &string_src,
            "using System.Text;",
            "the System.Text using directive so Encoding.UTF8 resolves in the wrapper and WireReader",
        );
    }

    /// Regression: record-only string usage still needs `System.Text` in
    /// the main file because the shared `WireReader` / `WireWriter`
    /// helpers live there, not in the record file.
    #[test]
    fn emit_record_only_string_fields_import_system_text_in_main_file() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "person",
            false,
            vec![
                ("name", TypeExpr::String),
                ("age", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_person",
            vec![("p", TypeExpr::Record(RecordId::new("person")))],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("person"))),
        ));

        let output = emit_contract(&contract);
        let main_source = output
            .files
            .iter()
            .find(|f| f.file_name == "DemoLib.cs")
            .expect("DemoLib.cs")
            .source
            .as_str();

        assert_source_contains(
            main_source,
            "using System.Text;",
            "the main file needs System.Text when record string fields make WireWriter use Encoding.UTF8.GetBytes/GetByteCount",
        );
        assert_source_contains(
            main_source,
            "Marshal.PtrToStringUTF8",
            "WireReader string decode still lives in the main file for record-only string usage",
        );
    }

    /// Regression: `Vec<String>` params now encode through `WireWriter`
    /// using `Encoding.UTF8`, even when the contract has no direct string
    /// params or string-bearing records. The main file still needs
    /// `System.Text` for that generated path to compile.
    #[test]
    fn emit_vec_string_param_imports_system_text_in_main_file() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "vec_string_lengths",
            vec![("v", TypeExpr::Vec(Box::new(TypeExpr::String)))],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                PrimitiveType::U32,
            )))),
        ));

        let output = emit_contract(&contract);
        let main_source = output
            .files
            .iter()
            .find(|f| f.file_name == "DemoLib.cs")
            .expect("DemoLib.cs")
            .source
            .as_str();

        assert_source_contains(
            main_source,
            "using System.Text;",
            "the main file needs System.Text when Vec<String> params make WireWriter size/write code call Encoding.UTF8",
        );
        assert_source_contains(
            main_source,
            "WireWriter.EncodedArraySize(v, sizeItem0 => (4 + Encoding.UTF8.GetByteCount(sizeItem0)))",
            "the encoded Vec<String> param path uses Encoding.UTF8 inside the shared main-file helpers",
        );
    }

    /// The shared bounds check avoids `_pos + n` overflow on malformed
    /// large lengths and still routes failures through the backend's
    /// "corrupt wire" exception path.
    #[test]
    fn emit_wire_reader_require_uses_overflow_safe_guard() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo",
            vec![("v", TypeExpr::String)],
            ReturnDef::Value(TypeExpr::String),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "if (n < 0 || n > _length - _pos) throw new InvalidOperationException(\"corrupt wire: truncated \" + kind);",
            "WireReader.Require must compare against remaining bytes instead of overflowing `_pos + n`",
        );
    }

    // ----- Record tests -----

    /// Build a record with the given fields. `is_repr_c = true` lets the
    /// IR classify it as blittable when every field is a primitive.
    fn record_with_fields(id: &str, is_repr_c: bool, fields: Vec<(&str, TypeExpr)>) -> RecordDef {
        RecordDef {
            id: RecordId::new(id),
            is_repr_c,
            is_error: false,
            fields: fields
                .into_iter()
                .map(|(name, type_expr)| FieldDef {
                    name: FieldName::new(name),
                    type_expr,
                    doc: None,
                    default: None,
                })
                .collect(),
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    /// A blittable record (`#[repr(C)]`, all primitive fields) should get
    /// the `[StructLayout(LayoutKind.Sequential)]` attribute so the CLR
    /// lays it out the same way Rust does and can pass it by value across
    /// the P/Invoke boundary without any wire encoding.
    #[test]
    fn emit_blittable_record_gets_struct_layout_attribute() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_point",
            vec![("p", TypeExpr::Record(RecordId::new("point")))],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("point"))),
        ));

        let output = emit_contract(&contract);
        let src = output.combined_source();

        assert_source_contains(
            &src,
            "[StructLayout(LayoutKind.Sequential)]",
            "Sequential layout attribute so Rust's #[repr(C)] layout matches the C# struct",
        );
        assert_source_contains(
            &src,
            "public readonly record struct Point(",
            "readonly record struct declaration: value type with generated equality",
        );
    }

    /// A blittable record used as a function param/return must pass
    /// directly across P/Invoke without any byte[] buffer or FfiBuf. The
    /// wrapper should be a one-liner forwarding to NativeMethods; the
    /// native signature should use the struct type.
    #[test]
    fn emit_blittable_record_passes_by_value_across_p_invoke() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_point",
            vec![("p", TypeExpr::Record(RecordId::new("point")))],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("point"))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Point EchoPoint(Point p)",
            "wrapper exposes the blittable record directly",
        );
        assert_source_contains(
            &src,
            "return NativeMethods.EchoPoint(p);",
            "single-line delegating body, no WireWriter, no FfiBuf",
        );
        assert_source_contains(
            &src,
            "internal static extern Point EchoPoint(Point p);",
            "DllImport takes and returns the struct directly",
        );
        assert_source_lacks(
            &src,
            "WireWriter(p.WireEncodedSize())",
            "no WireWriter setup for a blittable param (that would defeat the zero-copy win)",
        );
    }

    /// Each pinned record-array param needs its own `fixed` statement.
    /// C# rejects comma-joined declarations here, so the wrapper must
    /// nest the blocks when a function takes multiple
    /// `Vec<BlittableRecord>` params.
    #[test]
    fn emit_blittable_record_vec_params_use_nested_fixed_blocks() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.catalog.insert_record(record_with_fields(
            "color",
            true,
            vec![
                ("r", TypeExpr::Primitive(PrimitiveType::U8)),
                ("g", TypeExpr::Primitive(PrimitiveType::U8)),
                ("b", TypeExpr::Primitive(PrimitiveType::U8)),
                ("a", TypeExpr::Primitive(PrimitiveType::U8)),
            ],
        ));
        contract.functions.push(function_with_types(
            "score_batches",
            vec![
                (
                    "points",
                    TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("point")))),
                ),
                (
                    "colors",
                    TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("color")))),
                ),
            ],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "fixed (Point* _pointsPtr = points)",
            "the first pinned record vec param to get its own fixed statement",
        );
        assert_source_contains(
            &src,
            "fixed (Color* _colorsPtr = colors)",
            "the second pinned record vec param to get a nested fixed statement instead of a comma-joined declaration",
        );
        assert_source_lacks(
            &src,
            "fixed (Point* _pointsPtr = points, Color* _colorsPtr = colors)",
            "C# does not accept comma-joined fixed declarations across pinned params",
        );
        assert_source_contains(
            &src,
            "return NativeMethods.ScoreBatches((IntPtr)_pointsPtr, (UIntPtr)(points.Length * Unsafe.SizeOf<Point>()), (IntPtr)_colorsPtr, (UIntPtr)(colors.Length * Unsafe.SizeOf<Color>()));",
            "the native call to use both pointer locals and byte lengths from the nested fixed blocks",
        );
        assert_source_contains(
            &src,
            "internal static extern int ScoreBatches(IntPtr points, UIntPtr pointsLen, IntPtr colors, UIntPtr colorsLen);",
            "the DllImport signature to expose both pinned arrays as raw pointers plus byte lengths",
        );
    }

    /// Record methods share the same callable param machinery as top-level
    /// functions and class methods. A string param lowers to `Utf8Bytes`,
    /// so the record method body must create the `_prefixBytes` setup local
    /// before using it in the native call.
    #[test]
    fn emit_record_method_with_string_param_declares_utf8_setup_local() {
        let mut record =
            record_with_fields("service_config", false, vec![("name", TypeExpr::String)]);
        record.methods.push(MethodDef {
            id: MethodId::new("describe_with_prefix"),
            receiver: Receiver::RefSelf,
            params: vec![ParamDef {
                name: ParamName::new("prefix"),
                type_expr: TypeExpr::String,
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::String),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let mut contract = empty_contract();
        contract.catalog.insert_record(record);

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "byte[] _prefixBytes = Encoding.UTF8.GetBytes(prefix);",
            "record method string params to allocate the UTF-8 byte buffer before the native call",
        );
        assert_source_contains(
            &src,
            "FfiBuf _buf = NativeMethods.ServiceConfigDescribeWithPrefix(_selfBytes, (UIntPtr)_selfBytes.Length, _prefixBytes, (UIntPtr)_prefixBytes.Length);",
            "the native call to pass both encoded self and encoded string param",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf ServiceConfigDescribeWithPrefix(byte[] self, UIntPtr selfLen, byte[] prefix, UIntPtr prefixLen);",
            "the DllImport signature to split the receiver and string param into byte[]+UIntPtr pairs",
        );
    }

    /// A record method with a blittable record vec param emits
    /// `Unsafe.SizeOf<T>()` in that record's own `.cs` file. The per-record
    /// template therefore needs the same `System.Runtime.CompilerServices`
    /// import that functions/classes already conditionally emit.
    #[test]
    fn emit_record_method_with_blittable_record_vec_param_imports_unsafe_helpers() {
        let mut record = record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        );
        record.methods.push(MethodDef {
            id: MethodId::new("path_length"),
            receiver: Receiver::Static,
            params: vec![ParamDef {
                name: ParamName::new("points"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("point")))),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::F64)),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let mut contract = empty_contract();
        contract.catalog.insert_record(record);

        let output = emit_contract(&contract);
        let point = output
            .files
            .iter()
            .find(|f| f.file_name == "Point.cs")
            .expect("Point.cs record output");

        assert_source_contains(
            &point.source,
            "using System.Runtime.CompilerServices;",
            "the record file to import Unsafe when a record method has a pinned array param",
        );
        assert_source_contains(
            &point.source,
            "fixed (Point* _pointsPtr = points)",
            "the method body to pin the managed Point[] before the native call",
        );
        assert_source_contains(
            &point.source,
            "return NativeMethods.PointPathLength((IntPtr)_pointsPtr, (UIntPtr)(points.Length * Unsafe.SizeOf<Point>()));",
            "the native call to pass the pinned pointer and byte length",
        );
    }

    /// A non-blittable record (one with a string field) must NOT carry
    /// `[StructLayout(Sequential)]`. Its memory layout doesn't need to
    /// match Rust's because it travels as wire-encoded bytes, not as a
    /// struct value.
    #[test]
    fn emit_non_blittable_record_omits_struct_layout_attribute() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "person",
            false,
            vec![
                ("name", TypeExpr::String),
                ("age", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
        ));

        let output = emit_contract(&contract);
        let person_source = output
            .files
            .iter()
            .find(|f| f.file_name == "Person.cs")
            .expect("Person.cs")
            .source
            .as_str();

        assert!(
            !person_source.contains("[StructLayout"),
            "non-blittable record should not carry Sequential layout, but got:\n{person_source}"
        );
        assert!(
            person_source.contains("public readonly record struct Person("),
            "still a record struct just without the layout attribute"
        );
    }

    /// A non-blittable record param travels as a wire-encoded byte array.
    /// The wrapper must: (a) open a `using` WireWriter scoped to the
    /// buffer's lifetime, (b) call the record's `WireEncodeTo`, (c) grab
    /// the bytes, (d) pass `(byte[], UIntPtr)` to native, (e) decode the
    /// return and free the FfiBuf.
    #[test]
    fn emit_non_blittable_record_param_uses_wire_writer_and_byte_array() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "person",
            false,
            vec![
                ("name", TypeExpr::String),
                ("age", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_person",
            vec![("p", TypeExpr::Record(RecordId::new("person")))],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("person"))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "using var _wire_p = new WireWriter(p.WireEncodedSize());",
            "WireWriter rented with the record's exact encoded size, disposed at scope end",
        );
        assert_source_contains(
            &src,
            "p.WireEncodeTo(_wire_p);",
            "record encodes itself into the WireWriter via its generated method",
        );
        assert_source_contains(
            &src,
            "byte[] _pBytes = _wire_p.ToArray();",
            "bytes materialized before the native call",
        );
        assert_source_contains(
            &src,
            "FfiBuf _buf = NativeMethods.EchoPerson(_pBytes, (UIntPtr)_pBytes.Length);",
            "native call hands the (byte[], UIntPtr) pair",
        );
        assert_source_contains(
            &src,
            "return Person.Decode(new WireReader(_buf));",
            "return decodes the FfiBuf via the record's Decode method",
        );
        assert_source_contains(
            &src,
            "NativeMethods.FreeBuf(_buf);",
            "FreeBuf in finally so Rust reclaims the buffer even on decode failure",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoPerson(byte[] p, UIntPtr pLen);",
            "DllImport signature splits the record into (byte[], UIntPtr) and returns FfiBuf",
        );
    }

    /// A nested record's `WireEncodeTo` must delegate to the inner
    /// record's `WireEncodeTo` via the field accessor, and its `Decode`
    /// must call the inner record's `Decode`. This is the recursive
    /// glue that lets records contain records.
    #[test]
    fn emit_nested_record_encode_decode_delegates_to_inner_record() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "inner",
            false,
            vec![("label", TypeExpr::String)],
        ));
        contract.catalog.insert_record(record_with_fields(
            "outer",
            false,
            vec![("inner", TypeExpr::Record(RecordId::new("inner")))],
        ));

        let output = emit_contract(&contract);
        let outer = output
            .files
            .iter()
            .find(|f| f.file_name == "Outer.cs")
            .expect("Outer.cs")
            .source
            .as_str();

        assert!(
            outer.contains("Inner.Decode(reader)"),
            "nested field decode walks into the inner record's Decode, but Outer.cs was:\n{outer}"
        );
        assert!(
            outer.contains("this.Inner.WireEncodeTo(wire);"),
            "nested field encode walks into the inner record's WireEncodeTo, but Outer.cs was:\n{outer}"
        );
    }

    /// Record files should only import `System.Text` when a string field
    /// is present (needed for `Encoding.UTF8.GetByteCount` in the size
    /// expression). `TreatWarningsAsErrors` in downstream projects flags
    /// unused usings, so a blittable-only record must stay clean.
    #[test]
    fn emit_record_imports_system_text_only_when_string_fields_present() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.catalog.insert_record(record_with_fields(
            "person",
            false,
            vec![
                ("name", TypeExpr::String),
                ("age", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
        ));

        let output = emit_contract(&contract);
        let point = output
            .files
            .iter()
            .find(|f| f.file_name == "Point.cs")
            .unwrap()
            .source
            .as_str();
        let person = output
            .files
            .iter()
            .find(|f| f.file_name == "Person.cs")
            .unwrap()
            .source
            .as_str();

        assert!(
            !point.contains("using System.Text;"),
            "Point.cs (blittable, no strings) should not import System.Text"
        );
        assert!(
            person.contains("using System.Text;"),
            "Person.cs (has string field) needs System.Text for Encoding.UTF8.GetByteCount"
        );
        // And the inverse: StructLayout's using stays on blittable only.
        assert!(
            point.contains("using System.Runtime.InteropServices;"),
            "Point.cs uses StructLayout so it imports InteropServices"
        );
        assert!(
            !person.contains("using System.Runtime.InteropServices;"),
            "Person.cs has no StructLayout so it should not import InteropServices"
        );
    }

    /// Functions that mix string and non-string params must only emit the
    /// UTF-8 prep line for the string args and pass non-string args through
    /// unchanged.
    #[test]
    fn emit_mixed_string_and_primitive_params_only_encodes_strings() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "repeat_string",
            vec![
                ("v", TypeExpr::String),
                ("count", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
            ReturnDef::Value(TypeExpr::String),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "byte[] _vBytes = Encoding.UTF8.GetBytes(v);",
            "UTF-8 encoding only for the string param `v`",
        );
        assert_source_lacks(
            &src,
            "Encoding.UTF8.GetBytes(count)",
            "no UTF-8 encoding for the primitive `count` param",
        );
        assert_source_contains(
            &src,
            "NativeMethods.RepeatString(_vBytes, (UIntPtr)_vBytes.Length, count)",
            "the native call to expand only the string into (bytes, length) and pass the primitive through unchanged",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf RepeatString(byte[] v, UIntPtr vLen, uint count);",
            "the P/Invoke signature to split only the string into byte[]+UIntPtr, keeping the primitive uint direct",
        );
    }

    fn c_style_enum_with_tag_type(
        id: &str,
        tag_type: PrimitiveType,
        variants: Vec<&str>,
    ) -> EnumDef {
        EnumDef {
            id: EnumId::new(id),
            repr: EnumRepr::CStyle {
                tag_type,
                variants: variants
                    .into_iter()
                    .enumerate()
                    .map(|(i, name)| CStyleVariant {
                        name: name.into(),
                        discriminant: i as i128,
                        doc: None,
                    })
                    .collect(),
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    fn c_style_enum(id: &str, variants: Vec<&str>) -> EnumDef {
        c_style_enum_with_tag_type(id, PrimitiveType::I32, variants)
    }

    fn emit_files_for(contract: &FfiContract) -> Vec<(String, String)> {
        let output = emit_contract(contract);
        output
            .files
            .into_iter()
            .map(|f| (f.file_name, f.source))
            .collect()
    }

    /// A `#[repr(C)]` record whose fields are primitives + C-style enums
    /// keeps the zero-copy `[StructLayout(Sequential)]` path even though
    /// the IR's own blittability check (which predates enum support) says
    /// otherwise. The C# backend extends the rule locally because the CLR
    /// lays fixed-width `enum : T` values out bit-for-bit identically to
    /// their declared integral backing type.
    #[test]
    fn emit_repr_c_record_with_c_style_enum_field_stays_blittable() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_enum(c_style_enum("status", vec!["Active", "Inactive"]));
        contract.catalog.insert_record(record_with_fields(
            "flag",
            true,
            vec![
                ("status", TypeExpr::Enum(EnumId::new("status"))),
                ("count", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "[StructLayout(LayoutKind.Sequential)]",
            "the zero-copy struct-layout attribute when only primitive + C-style enum fields are present",
        );
        assert_source_contains(
            &src,
            "public readonly record struct Flag(",
            "the C# record-struct declaration",
        );
        assert_source_contains(
            &src,
            "Status Status,",
            "the enum field rendered with its C# enum type name, not the backing int",
        );
    }

    fn data_enum_single_variant(id: &str, variant_name: &str, field: (&str, TypeExpr)) -> EnumDef {
        EnumDef {
            id: EnumId::new(id),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: variant_name.into(),
                    discriminant: 0,
                    payload: VariantPayload::Struct(vec![FieldDef {
                        name: field.0.into(),
                        type_expr: field.1,
                        doc: None,
                        default: None,
                    }]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        }
    }

    /// A function taking and returning a data enum travels through the
    /// wire codec just like a non-blittable record. The public wrapper
    /// allocates a `WireWriter`, encodes the input, calls the native
    /// DllImport with `(byte[], UIntPtr)`, and decodes the returned
    /// `FfiBuf` via `Shape.Decode(new WireReader(_buf))`. Same shape as
    /// the record path, one rendering.
    #[test]
    fn emit_function_with_data_enum_param_and_return_goes_through_wire() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(data_enum_single_variant(
            "shape",
            "Circle",
            ("radius", TypeExpr::Primitive(PrimitiveType::F64)),
        ));
        contract.functions.push(function_with_types(
            "echo_shape",
            vec![("s", TypeExpr::Enum(EnumId::new("shape")))],
            ReturnDef::Value(TypeExpr::Enum(EnumId::new("shape"))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Shape EchoShape(Shape s)",
            "the public wrapper signature to name the Shape data enum on both sides",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoShape(byte[] s, UIntPtr sLen);",
            "the DllImport signature to split the data enum param into (byte[], UIntPtr) and return an FfiBuf",
        );
        assert_source_contains(
            &src,
            "using var _wire_s = new WireWriter(s.WireEncodedSize());",
            "the wrapper body to allocate a WireWriter sized to the input value before the native call",
        );
        assert_source_contains(
            &src,
            "s.WireEncodeTo(_wire_s);",
            "the wrapper body to drive the data enum's own WireEncodeTo: same call shape as records",
        );
        assert_source_contains(
            &src,
            "return Shape.Decode(new WireReader(_buf));",
            "the wrapper body to decode the returned FfiBuf through the data enum's static Decode",
        );
    }

    /// An instance method on a C-style enum travels as a C# extension
    /// method `{Name}(this Direction self, …)` with `self` passed
    /// directly to the DllImport. The DllImport's C# method name gets
    /// the enum-class prefix so it doesn't collide with same-named
    /// methods on other enums.
    #[test]
    fn emit_c_style_enum_instance_method_renders_as_extension_with_prefixed_native_name() {
        let mut enum_def = c_style_enum("direction", vec!["North", "South"]);
        enum_def.methods.push(MethodDef {
            id: MethodId::new("opposite"),
            receiver: Receiver::RefSelf,
            params: vec![],
            returns: ReturnDef::Value(TypeExpr::Enum(EnumId::new("direction"))),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });

        let mut contract = empty_contract();
        contract.catalog.insert_enum(enum_def);

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Direction Opposite(this Direction self)",
            "an instance method on a C-style enum to render as a C# extension method",
        );
        assert_source_contains(
            &src,
            "return NativeMethods.DirectionOpposite(self);",
            "the extension-method body to call the prefixed native entry with `self` passed directly",
        );
        assert_source_contains(
            &src,
            "internal static extern Direction DirectionOpposite(Direction self);",
            "the DllImport to declare the prefixed native name, return the enum type directly, and take the enum-typed self param",
        );
    }

    /// Enum methods share the same value-type method template as enum
    /// constructors. A blittable record vec param therefore needs the
    /// same `unsafe { fixed (...) { ... } }` wrapper as a top-level
    /// function so the generated pointer local exists at the native call
    /// site and the array stays pinned for the duration of the call.
    #[test]
    fn emit_enum_method_with_blittable_record_vec_param_uses_fixed_block() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        let mut enum_def = c_style_enum("direction", vec!["North", "South"]);
        enum_def.methods.push(MethodDef {
            id: MethodId::new("from_points"),
            receiver: Receiver::Static,
            params: vec![ParamDef {
                name: ParamName::new("points"),
                type_expr: TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("point")))),
                passing: ParamPassing::Value,
                doc: None,
            }],
            returns: ReturnDef::Value(TypeExpr::Enum(EnumId::new("direction"))),
            execution_kind: ExecutionKind::Sync,
            doc: None,
            deprecated: None,
        });
        contract.catalog.insert_enum(enum_def);

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Direction FromPoints(Point[] points)",
            "the enum companion method to expose the blittable record vec as Point[]",
        );
        assert_source_contains(
            &src,
            "fixed (Point* _pointsPtr = points)",
            "the method body to pin the managed Point[] before the native call",
        );
        assert_source_contains(
            &src,
            "return NativeMethods.DirectionFromPoints((IntPtr)_pointsPtr, (UIntPtr)(points.Length * Unsafe.SizeOf<Point>()));",
            "the native call to use the pointer local introduced by the fixed block",
        );
        assert_source_contains(
            &src,
            "internal static extern Direction DirectionFromPoints(IntPtr points, UIntPtr pointsLen);",
            "the DllImport signature to take a raw pointer and byte length for the pinned array param",
        );
    }

    /// A function that takes and returns a C-style enum marshals through
    /// P/Invoke with zero ceremony. The DllImport signature names the
    /// enum type directly, and the public wrapper is a one-line pass-
    /// through. No cast, no byte buffer, no FfiBuf.
    #[test]
    fn emit_function_with_c_style_enum_param_and_return_marshals_direct() {
        let mut contract = empty_contract();
        contract
            .catalog
            .insert_enum(c_style_enum("status", vec!["Active", "Inactive"]));
        contract.functions.push(function_with_types(
            "echo_status",
            vec![("s", TypeExpr::Enum(EnumId::new("status")))],
            ReturnDef::Value(TypeExpr::Enum(EnumId::new("status"))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Status EchoStatus(Status s)",
            "the public wrapper signature to name the Status enum on both sides, not the backing int",
        );
        assert_source_contains(
            &src,
            "internal static extern Status EchoStatus(Status s);",
            "the DllImport signature to declare the enum type directly so the CLR marshals it transparently as its backing int",
        );
        assert_source_contains(
            &src,
            "return NativeMethods.EchoStatus(s);",
            "the wrapper body to pass the enum through unchanged, no cast required",
        );
        assert_source_lacks(
            &src,
            "(int)s",
            "no explicit int cast since the CLR handles enum marshaling",
        );
    }

    /// A C-style enum in the catalog produces its own `.cs` file containing
    /// both the native `enum` declaration and the `Wire` helper class used
    /// when the enum is embedded in a wire-encoded context.
    #[test]
    fn emit_c_style_enum_produces_per_enum_file() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(c_style_enum(
            "status",
            vec!["Active", "Inactive", "Pending"],
        ));

        let files = emit_files_for(&contract);

        let status_cs = files
            .iter()
            .find(|(name, _)| name == "Status.cs")
            .expect("expecting Status.cs to be generated for the status enum");
        assert_source_contains(
            &status_cs.1,
            "public enum Status : int",
            "the native C# enum declaration with explicit int backing type",
        );
        assert_source_contains(
            &status_cs.1,
            "Active = 0",
            "variant tags as ordinal indices matching EnumTagStrategy::OrdinalIndex",
        );
        assert_source_contains(
            &status_cs.1,
            "internal static class StatusWire",
            "the paired static helper class with Decode and the WireEncodeTo extension",
        );
    }

    /// A `#[repr(u8)]` enum's public declaration uses `byte` as the backing
    /// type (so the CLR marshals the enum as its declared width on the
    /// direct-P/Invoke path), but the wire codec stays on the 4-byte
    /// `i32` ordinal format every boltffi backend agrees on. Mixing the
    /// two would cause a cross-language byte-count mismatch in
    /// wire-encoded containers.
    #[test]
    fn emit_u8_c_style_enum_declares_byte_backing_but_uses_4_byte_i32_wire_codec() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(c_style_enum_with_tag_type(
            "log_level",
            PrimitiveType::U8,
            vec!["Trace", "Debug", "Info", "Warn", "Error"],
        ));

        let files = emit_files_for(&contract);

        let log_level_cs = files
            .iter()
            .find(|(name, _)| name == "LogLevel.cs")
            .expect("expecting LogLevel.cs to be generated for the log_level enum");
        assert_source_contains(
            &log_level_cs.1,
            "public enum LogLevel : byte",
            "the native C# enum declaration with the repr(u8) backing type preserved",
        );
        assert_source_contains(
            &log_level_cs.1,
            "internal const int WireEncodedSize = 4;",
            "the wire codec to use the cross-backend 4-byte i32 ordinal format",
        );
        assert_source_contains(
            &log_level_cs.1,
            "reader.ReadI32() switch",
            "the decode helper to read a 4-byte i32 ordinal and switch on it",
        );
        assert_source_contains(
            &log_level_cs.1,
            "wire.WriteI32(value switch",
            "the encode helper to write a 4-byte i32 after mapping the variant to its ordinal",
        );
    }

    // ----- Encoded Vec tests (Vec<String>, Vec<Vec<_>>) -----

    /// `Vec<String>` as a param travels wire-encoded: a `WireWriter` sized
    /// via `EncodedArraySize` writes a length-prefixed array of
    /// length-prefixed UTF-8 strings. As a return it comes back through
    /// `ReadEncodedArray<string>` wrapping `ReadString`.
    #[test]
    fn emit_vec_string_round_trips_through_encoded_array_helpers() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_vec_string",
            vec![("v", TypeExpr::Vec(Box::new(TypeExpr::String)))],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::String))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static string[] EchoVecString(string[] v)",
            "the public wrapper exposes Vec<String> on both sides as string[]",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoVecString(byte[] v, UIntPtr vLen);",
            "the DllImport carries the wire-encoded buffer, not a raw string[]",
        );
        assert_source_contains(
            &src,
            "WireWriter.EncodedArraySize(v, sizeItem0 => (4 + Encoding.UTF8.GetByteCount(sizeItem0)))",
            "the WireWriter size hint uses EncodedArraySize with a per-element UTF-8 byte-count lambda",
        );
        assert_source_contains(
            &src,
            "_wire_v.WriteI32(v.Length);",
            "the encode body writes the 4-byte count first",
        );
        assert_source_contains(
            &src,
            "foreach (string item0 in v) { _wire_v.WriteString(item0); }",
            "the encode body then loops WriteString over each element",
        );
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<string>(r0 => r0.ReadString());",
            "the return decodes through ReadEncodedArray with a ReadString closure per element",
        );
    }

    /// `Vec<Vec<i32>>` exercises the nested-encoded-over-blittable path:
    /// outer layer is wire-encoded (count prefix + per-element bytes),
    /// inner layer is length-prefixed blittable. Loop variables must be
    /// unique across nesting (`item0` for the outer write, `item1` for
    /// the inner) so inner references don't shadow the outer.
    #[test]
    fn emit_vec_vec_i32_nests_blittable_inside_encoded() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_vec_vec_i32",
            vec![(
                "v",
                TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(
                TypeExpr::Primitive(PrimitiveType::I32),
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static int[][] EchoVecVecI32(int[][] v)",
            "the public wrapper exposes Vec<Vec<i32>> as a jagged int[][]",
        );
        assert_source_contains(
            &src,
            "_wire_v.WriteI32(v.Length);",
            "the outer write emits the count first",
        );
        assert_source_contains(
            &src,
            "foreach (int[] item0 in v) { _wire_v.WriteBlittableArray(item0); }",
            "then loops WriteBlittableArray (which writes its own length prefix) over each inner array",
        );
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<int[]>(r0 => r0.ReadLengthPrefixedBlittableArray<int>());",
            "the return decodes through ReadEncodedArray wrapping a nested ReadLengthPrefixedBlittableArray",
        );
    }

    /// `Vec<Vec<String>>` doubles up the encoded path: both layers carry a
    /// 4-byte count prefix, and the inner element is itself variable-width.
    /// The decode closure name (`r1`) and inner closure (`r0`) must differ
    /// so scopes don't shadow; the same property holds for write loop vars
    /// (`item0` outer, `item1` inner).
    #[test]
    fn emit_vec_vec_string_doubles_the_encoded_array_path() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_vec_vec_string",
            vec![(
                "v",
                TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(TypeExpr::String)))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(
                TypeExpr::String,
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static string[][] EchoVecVecString(string[][] v)",
            "the public wrapper exposes Vec<Vec<String>> as a jagged string[][]",
        );
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<string[]>(r0 => r0.ReadEncodedArray<string>(r1 => r1.ReadString()));",
            "the return decodes through two nested ReadEncodedArray closures, outer-first numbering",
        );
        assert_source_contains(
            &src,
            "_wire_v.WriteI32(v.Length);",
            "the outer encode writes the outer length first",
        );
        assert_source_contains(
            &src,
            "foreach (string[] item0 in v) { _wire_v.WriteI32(item0.Length); foreach (string item1 in item0) { _wire_v.WriteString(item1); }; }",
            "then nests two foreach loops with distinct loop variables",
        );
    }

    /// Regression: when a data-enum variant field is `Vec<Vec<String>>`,
    /// the `_v` prefix rewrite must apply only to the outer field access
    /// (`_v.Groups`) and must leave the nested loop / lambda bindings
    /// alone. Rewriting the inner references to `_v.item1` or `_v.item0`
    /// would break both the size expression and the encode loop.
    #[test]
    fn emit_data_enum_variant_nested_vec_string_prefixes_only_outer_field_access() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(data_enum_single_variant(
            "filter",
            "ByGroups",
            (
                "groups",
                TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(TypeExpr::String)))),
            ),
        ));
        contract.functions.push(function_with_types(
            "echo_filter",
            vec![("f", TypeExpr::Enum(EnumId::new("filter")))],
            ReturnDef::Value(TypeExpr::Enum(EnumId::new("filter"))),
        ));

        let files = emit_files_for(&contract);
        let enum_src = files
            .iter()
            .find(|(name, _)| name == "Filter.cs")
            .expect("Filter.cs")
            .1
            .as_str();

        assert_source_contains(
            enum_src,
            "ByGroups _v => WireWriter.EncodedArraySize(_v.Groups, sizeItem0 => WireWriter.EncodedArraySize(sizeItem0, sizeItem1 => (4 + Encoding.UTF8.GetByteCount(sizeItem1))))",
            "the size expression to prefix only the outer field access and keep distinct nested lambda variables",
        );
        assert_source_contains(
            enum_src,
            "wire.WriteI32(_v.Groups.Length);",
            "the outer encode writes the count using the variant-bound `_v` access",
        );
        assert_source_contains(
            enum_src,
            "foreach (string[] item0 in _v.Groups) { wire.WriteI32(item0.Length); foreach (string item1 in item0) { wire.WriteString(item1); }; }",
            "then nests two foreach loops with distinct loop variables",
        );
        assert_source_lacks(
            enum_src,
            "_v.item1",
            "the outer `_v` prefix must not leak into the nested foreach binding",
        );
        assert_source_lacks(
            enum_src,
            "_v.item0",
            "the outer `_v` prefix must not leak into the innermost foreach binding",
        );
        assert_source_lacks(
            enum_src,
            "_v.sizeItem1",
            "the outer `_v` prefix must not leak into the nested size lambda binding",
        );
        assert_source_lacks(
            enum_src,
            "_v.sizeItem0",
            "the outer `_v` prefix must not leak into the innermost size lambda binding",
        );
    }

    /// A function that returns `Vec<i32>` by flattening a `Vec<Vec<i32>>`
    /// param keeps the top-level return on the no-prefix blittable fast
    /// path: the outer count comes from `FfiBuf.len`. This guards against
    /// regressions that would route the return through
    /// `ReadEncodedArray` or add a spurious length-prefixed helper.
    #[test]
    fn emit_flatten_vec_vec_i32_keeps_top_level_return_on_blittable_fast_path() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "flatten_vec_vec_i32",
            vec![(
                "v",
                TypeExpr::Vec(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadBlittableArray<int>();",
            "the top-level Vec<i32> return stays on the no-prefix fast path, count taken from FfiBuf.len",
        );
    }

    // ----- Encoded Vec tests (Vec<Enum>, Vec<non-blittable Record>) -----

    /// `Vec<CStyleEnum>` rides the wire-encoded path on both sides because
    /// the Rust `#[export]` macro classifies C-style enums as `Scalar`
    /// and its `supports_direct_vec` gate only admits `Blittable`. A
    /// bulk-copy fast path would hand Rust raw enum bytes where it
    /// expects a length-prefixed array of I32 tags. The generated
    /// wrapper should encode via `{Name}Wire.WireEncodeTo` per element
    /// and decode via `ReadEncodedArray<{Name}>(r => {Name}Wire.Decode(r))`.
    #[test]
    fn emit_vec_c_style_enum_round_trips_through_encoded_array_helpers() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(c_style_enum(
            "status",
            vec!["Active", "Inactive", "Pending"],
        ));
        contract.functions.push(function_with_types(
            "echo_vec_status",
            vec![(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Enum(EnumId::new("status")))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Enum(EnumId::new(
                "status",
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Status[] EchoVecStatus(Status[] values)",
            "the public wrapper exposes Vec<Status> on both sides as Status[]",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoVecStatus(byte[] values, UIntPtr valuesLen);",
            "the DllImport carries the wire-encoded buffer, matching the macro's WireEncoded classification for Vec<Scalar>",
        );
        assert_source_contains(
            &src,
            "_wire_values.WriteI32(values.Length);",
            "the encode body writes the 4-byte count first",
        );
        assert_source_contains(
            &src,
            "foreach (Status item0 in values) { item0.WireEncodeTo(_wire_values); }",
            "then loops WireEncodeTo over each enum value",
        );
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<Status>(r0 => StatusWire.Decode(r0));",
            "the return decodes through ReadEncodedArray with the StatusWire.Decode helper per element",
        );
    }

    /// `Vec<DataEnum>` rides the wire-encoded path. Each element carries
    /// its own variant tag + payload, so the encode loop delegates to
    /// the enum's own `WireEncodeTo` and decode delegates to its
    /// `Decode` static. Same call shape as `Vec<CStyleEnum>` but the
    /// inner decode is the data-enum entry point (`Shape.Decode`) rather
    /// than the `Wire` helper.
    #[test]
    fn emit_vec_data_enum_round_trips_through_encoded_array_helpers() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(data_enum_single_variant(
            "shape",
            "Circle",
            ("radius", TypeExpr::Primitive(PrimitiveType::F64)),
        ));
        contract.functions.push(function_with_types(
            "echo_vec_shape",
            vec![(
                "values",
                TypeExpr::Vec(Box::new(TypeExpr::Enum(EnumId::new("shape")))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Enum(EnumId::new(
                "shape",
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Shape[] EchoVecShape(Shape[] values)",
            "the public wrapper exposes Vec<Shape> on both sides as Shape[]",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoVecShape(byte[] values, UIntPtr valuesLen);",
            "the DllImport takes a wire-encoded buffer and returns an FfiBuf",
        );
        assert_source_contains(
            &src,
            "_wire_values.WriteI32(values.Length);",
            "the encode body writes the count first",
        );
        assert_source_contains(
            &src,
            "foreach (Shape item0 in values) { item0.WireEncodeTo(_wire_values); }",
            "then loops the data enum's WireEncodeTo over each element",
        );
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<Shape>(r0 => Shape.Decode(r0));",
            "the return decodes through ReadEncodedArray with Shape.Decode per element",
        );
    }

    /// `Vec<NonBlittableRecord>` rides the wire-encoded path: the record
    /// carries a string field, so each element is a variable-width
    /// payload that serialises via the record's own `WireEncodeTo` and
    /// deserialises via its `Decode` static. Guards against regressions
    /// that would route non-blittable record vecs onto the pinned
    /// fast path (which only works for blittable records).
    #[test]
    fn emit_vec_non_blittable_record_round_trips_through_encoded_array_helpers() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "person",
            false,
            vec![
                ("name", TypeExpr::String),
                ("age", TypeExpr::Primitive(PrimitiveType::U32)),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_vec_person",
            vec![(
                "people",
                TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("person")))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new(
                "person",
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Person[] EchoVecPerson(Person[] people)",
            "the public wrapper exposes Vec<Person> on both sides as Person[]",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoVecPerson(byte[] people, UIntPtr peopleLen);",
            "the DllImport takes a wire-encoded buffer and returns an FfiBuf",
        );
        assert_source_contains(
            &src,
            "_wire_people.WriteI32(people.Length);",
            "the encode body writes the count first",
        );
        assert_source_contains(
            &src,
            "foreach (Person item0 in people) { item0.WireEncodeTo(_wire_people); }",
            "then loops the record's WireEncodeTo over each element",
        );
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<Person>(r0 => Person.Decode(r0));",
            "the return decodes through ReadEncodedArray with Person.Decode per element",
        );
        assert_source_lacks(
            &src,
            "fixed (Person*",
            "non-blittable record vecs should not go through the pinned fast path",
        );
    }

    /// `Polygon { points: Vec<Point> }` is the canonical record-with-
    /// blittable-Vec-field shape. The field rides the length-prefixed
    /// blittable path inside the enclosing record's wire buffer: the
    /// codec emits `wire.WriteBlittableArray(this.Points)` on write
    /// (which produces the 4-byte count + raw element bytes) and
    /// `reader.ReadLengthPrefixedBlittableArray<Point>()` on read. The
    /// size contribution is `(4 + this.Points.Length * 16)` because
    /// Point is 16 bytes wide.
    #[test]
    fn emit_record_with_blittable_vec_field_uses_length_prefixed_blittable_codec() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.catalog.insert_record(record_with_fields(
            "polygon",
            false,
            vec![(
                "points",
                TypeExpr::Vec(Box::new(TypeExpr::Record(RecordId::new("point")))),
            )],
        ));
        contract.functions.push(function_with_types(
            "echo_polygon",
            vec![("p", TypeExpr::Record(RecordId::new("polygon")))],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("polygon"))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "wire.WriteBlittableArray(this.Points)",
            "the record's encode body writes the Vec<Point> via WriteBlittableArray, \
             which emits the 4-byte count and the raw element bytes",
        );
        assert_source_contains(
            &src,
            "reader.ReadLengthPrefixedBlittableArray<Point>()",
            "the record's decode reads the Vec<Point> back through the length-prefixed blittable helper",
        );
        assert_source_contains(
            &src,
            "(4 + this.Points.Length * 16)",
            "the size expression accounts for the 4-byte length prefix and the element stride \
             (two f64s → 16 bytes per Point)",
        );
    }

    /// `echo_optional_i32(Option<i32>) -> Option<i32>` is the canonical
    /// Option-over-primitive shape. The public wrapper must expose `int?`
    /// on both sides; the wire codec must: (a) size-prefix with the 1-byte
    /// tag, (b) encode via the `is { } opt0` pattern binding so the
    /// unwrapped value is named once, and (c) decode with an explicit
    /// `(int?)null` cast on the null branch so the conditional's type
    /// resolves to `int?` instead of `int` or bare `null`.
    #[test]
    fn emit_option_primitive_round_trip_uses_tagged_wire_encoding() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_optional_i32",
            vec![(
                "v",
                TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::I32))),
            )],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static int? EchoOptionalI32(int? v)",
            "the public wrapper exposes the Option<i32> param and return as int? on both sides",
        );
        assert_source_contains(
            &src,
            "using var _wire_v = new WireWriter((1 + (v is { } sizeOpt0 ? 4 : 0)));",
            "the WireWriter is rented with 1 byte for the tag plus the inner size when present, \
             using the non-null pattern binding under a size-specific prefix so it doesn't \
             collide with the write-side `opt0` in the same method scope",
        );
        assert_source_contains(
            &src,
            "if (v is { } opt0) { _wire_v.WriteU8((byte)1); _wire_v.WriteI32(opt0); } \
             else { _wire_v.WriteU8((byte)0); }",
            "the encode body uses the non-null pattern binding to name the unwrapped value \
             once for both the tag write and the primitive write",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf EchoOptionalI32(byte[] v, UIntPtr vLen);",
            "the DllImport takes the option as a wire-encoded byte[] + length pair and \
             returns an FfiBuf carrying the tagged response",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); \
             return reader.ReadU8() == 0 ? (int?)null : reader.ReadI32();",
            "the return body binds a reader local, reads the 1-byte tag, and casts null on \
             the missing branch so the conditional resolves to int? rather than bare null",
        );
        assert_source_contains(
            &src,
            "NativeMethods.FreeBuf(_buf);",
            "the FfiBuf is freed in a finally block, same as every other wire-decoded return",
        );
    }

    /// `find_even(i32) -> Option<i32>` is the minimal "Option return, no
    /// Option param" shape. The param side stays direct (int passes by
    /// value across P/Invoke), but the return still rides the wire path
    /// because an Option's 1-byte tag + payload doesn't line up with any
    /// CLR primitive layout.
    #[test]
    fn emit_function_returning_option_primitive_keeps_direct_param_but_wires_return() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "find_even",
            vec![("value", TypeExpr::Primitive(PrimitiveType::I32))],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Primitive(
                PrimitiveType::I32,
            )))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static int? FindEven(int value)",
            "the wrapper exposes the i32 param directly and the Option<i32> return as int?",
        );
        assert_source_contains(
            &src,
            "internal static extern FfiBuf FindEven(int value);",
            "the DllImport keeps the i32 param direct and returns an FfiBuf for the tagged option",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); \
             return reader.ReadU8() == 0 ? (int?)null : reader.ReadI32();",
            "the return body reads the option tag and either returns null or the decoded i32",
        );
        assert_source_lacks(
            &src,
            "using var _wire_value",
            "a direct-param i32 should not get a WireWriter setup, even when the return is Option",
        );
    }

    /// Generated `.cs` files must opt in to `#nullable enable` so
    /// `int?` / `string?` compile under consumer projects that have
    /// `<TreatWarningsAsErrors>` turned on. Every file (preamble, record,
    /// C-style enum, data enum) carries the directive, so consumers are
    /// free to leave their own csproj on `<Nullable>disable</Nullable>`.
    #[test]
    fn emit_every_generated_file_opts_in_to_nullable_annotations() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![CStyleVariant {
                    name: "Active".into(),
                    discriminant: 0,
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![DataVariant {
                    name: "Circle".into(),
                    discriminant: 0,
                    payload: VariantPayload::Struct(vec![FieldDef {
                        name: FieldName::new("radius"),
                        type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                        doc: None,
                        default: None,
                    }]),
                    doc: None,
                }],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(primitive_function(
            "add",
            vec![("a", PrimitiveType::I32), ("b", PrimitiveType::I32)],
            ReturnDef::Value(TypeExpr::Primitive(PrimitiveType::I32)),
        ));

        let output = emit_contract(&contract);

        for file in &output.files {
            assert!(
                file.source.contains("#nullable enable"),
                "expecting #nullable enable in {} but not found:\n{}",
                file.file_name,
                file.source,
            );
        }
    }

    /// `Option<String>` exercises the variable-width inner: the size
    /// expression must include the 4-byte length prefix plus the UTF-8
    /// byte count of the unwrapped string, threaded through the same
    /// `sizeOpt0` pattern binding so the inner's `v` identifier resolves
    /// to the non-null value without recomputing the option.
    #[test]
    fn emit_option_string_renders_utf8_sized_wire_payload() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_optional_string",
            vec![("v", TypeExpr::Option(Box::new(TypeExpr::String)))],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::String))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static string? EchoOptionalString(string? v)",
            "Option<String> renders as string? on both sides under #nullable enable",
        );
        assert_source_contains(
            &src,
            "using var _wire_v = new WireWriter((1 + (v is { } sizeOpt0 ? (4 + Encoding.UTF8.GetByteCount(sizeOpt0)) : 0)));",
            "the size sums the 1-byte tag with the 4-byte length prefix and the payload's UTF-8 byte count",
        );
        assert_source_contains(
            &src,
            "if (v is { } opt0) { _wire_v.WriteU8((byte)1); _wire_v.WriteString(opt0); } else { _wire_v.WriteU8((byte)0); }",
            "the encode dispatches to WriteString on the unwrapped value",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); return reader.ReadU8() == 0 ? (string?)null : reader.ReadString();",
            "the decode casts the null branch to string? so the conditional resolves to the nullable reference type",
        );
    }

    /// `Option<BlittableRecord>` still rides the wire path. The 1-byte
    /// tag in front of the record forces encode/decode, even though the
    /// record itself is `#[repr(C)]` and could otherwise cross P/Invoke
    /// by value. Encode dispatches to the record's `WireEncodeTo`;
    /// decode to `Point.Decode`. The null-branch cast must be `(Point?)`.
    #[test]
    fn emit_option_blittable_record_writes_and_decodes_through_record_helpers() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "point",
            true,
            vec![
                ("x", TypeExpr::Primitive(PrimitiveType::F64)),
                ("y", TypeExpr::Primitive(PrimitiveType::F64)),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_optional_point",
            vec![(
                "v",
                TypeExpr::Option(Box::new(TypeExpr::Record(RecordId::new("point")))),
            )],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Record(RecordId::new(
                "point",
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Point? EchoOptionalPoint(Point? v)",
            "Option<Point> renders as Point?: value-type inner desugars to Nullable<Point>",
        );
        assert_source_contains(
            &src,
            "using var _wire_v = new WireWriter((1 + (v is { } sizeOpt0 ? 16 : 0)));",
            "Point is two f64 fields so the payload contributes a fixed 16 bytes after the 1-byte tag",
        );
        assert_source_contains(
            &src,
            "if (v is { } opt0) { _wire_v.WriteU8((byte)1); opt0.WireEncodeTo(_wire_v); } else { _wire_v.WriteU8((byte)0); }",
            "encode dispatches to the record's own WireEncodeTo on the unwrapped value",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); return reader.ReadU8() == 0 ? (Point?)null : Point.Decode(reader);",
            "decode casts the null branch to Point? and otherwise reconstructs through Point.Decode",
        );
    }

    /// `Option<CStyleEnum>` must route through the wire path because the
    /// 1-byte tag defeats direct P/Invoke marshaling. Encode calls the
    /// enum's `WireEncodeTo` extension method; decode calls
    /// `{Name}Wire.Decode`, the same helpers used when a C-style enum
    /// embeds inside a wire-encoded record.
    #[test]
    fn emit_option_c_style_enum_goes_through_wire_helpers() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("status"),
            repr: EnumRepr::CStyle {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    CStyleVariant {
                        name: "Active".into(),
                        discriminant: 0,
                        doc: None,
                    },
                    CStyleVariant {
                        name: "Inactive".into(),
                        discriminant: 1,
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function_with_types(
            "echo_optional_status",
            vec![(
                "v",
                TypeExpr::Option(Box::new(TypeExpr::Enum(EnumId::new("status")))),
            )],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Enum(EnumId::new(
                "status",
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Status? EchoOptionalStatus(Status? v)",
            "Option<Status> renders as Status?: C# enums are value types, so the nullable is Nullable<Status>",
        );
        assert_source_contains(
            &src,
            "if (v is { } opt0) { _wire_v.WriteU8((byte)1); opt0.WireEncodeTo(_wire_v); } else { _wire_v.WriteU8((byte)0); }",
            "encode dispatches to the StatusWire extension method on the unwrapped enum value",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); return reader.ReadU8() == 0 ? (Status?)null : StatusWire.Decode(reader);",
            "decode calls StatusWire.Decode on the Some branch, null-casts on the None branch",
        );
    }

    /// `Option<DataEnum>` returns an `{Name}?`, a nullable reference
    /// because the generated data enum is an `abstract record`. Decode
    /// dispatches to the enum's `Decode` static, which walks the wire
    /// tag through the variant switch inside the reader.
    #[test]
    fn emit_option_data_enum_decodes_through_enum_static_decode() {
        let mut contract = empty_contract();
        contract.catalog.insert_enum(EnumDef {
            id: EnumId::new("shape"),
            repr: EnumRepr::Data {
                tag_type: PrimitiveType::I32,
                variants: vec![
                    DataVariant {
                        name: "Circle".into(),
                        discriminant: 0,
                        payload: VariantPayload::Struct(vec![FieldDef {
                            name: FieldName::new("radius"),
                            type_expr: TypeExpr::Primitive(PrimitiveType::F64),
                            doc: None,
                            default: None,
                        }]),
                        doc: None,
                    },
                    DataVariant {
                        name: "Square".into(),
                        discriminant: 1,
                        payload: VariantPayload::Unit,
                        doc: None,
                    },
                ],
            },
            is_error: false,
            constructors: vec![],
            methods: vec![],
            doc: None,
            deprecated: None,
        });
        contract.functions.push(function_with_types(
            "find_shape",
            vec![("id", TypeExpr::Primitive(PrimitiveType::I32))],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Enum(EnumId::new(
                "shape",
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static Shape? FindShape(int id)",
            "Option<Shape> renders as Shape?: Shape is an abstract record, so `?` means nullable reference",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); return reader.ReadU8() == 0 ? (Shape?)null : Shape.Decode(reader);",
            "decode reads the present tag, then either null-casts or dispatches to the enum's Decode",
        );
    }

    /// A record with two Option fields exercises the shared-emit-context
    /// plumbing: both fields must pick fresh pattern-binding names so
    /// `WireEncodedSize` and `WireEncodeTo` stay legal inside one method
    /// scope. Without the shared context the second field would try to
    /// redeclare `sizeOpt0` / `opt0` and fail at compile time.
    #[test]
    fn emit_record_with_two_option_fields_uses_distinct_pattern_bindings() {
        let mut contract = empty_contract();
        contract.catalog.insert_record(record_with_fields(
            "user_profile",
            false,
            vec![
                ("name", TypeExpr::String),
                ("email", TypeExpr::Option(Box::new(TypeExpr::String))),
                (
                    "score",
                    TypeExpr::Option(Box::new(TypeExpr::Primitive(PrimitiveType::F64))),
                ),
            ],
        ));
        contract.functions.push(function_with_types(
            "echo_user_profile",
            vec![("profile", TypeExpr::Record(RecordId::new("user_profile")))],
            ReturnDef::Value(TypeExpr::Record(RecordId::new("user_profile"))),
        ));

        let output = emit_contract(&contract);
        let record_source = output
            .files
            .iter()
            .find(|f| f.file_name == "UserProfile.cs")
            .expect("UserProfile.cs")
            .source
            .as_str();

        // Both size bindings must appear with distinct indices; if the
        // shared context regressed they would both be `sizeOpt0`.
        assert_source_contains(
            record_source,
            "(1 + (this.Email is { } sizeOpt0 ? (4 + Encoding.UTF8.GetByteCount(sizeOpt0)) : 0)) +",
            "the first Option field's size contribution uses sizeOpt0",
        );
        assert_source_contains(
            record_source,
            "(1 + (this.Score is { } sizeOpt1 ? 8 : 0))",
            "the second Option field's size contribution advances to sizeOpt1, \
             confirming the shared emit context is threaded across sibling fields",
        );
        // Same story on the encode side: two Option fields, distinct
        // `opt0` / `opt1` pattern names.
        assert_source_contains(
            record_source,
            "if (this.Email is { } opt0) { wire.WriteU8((byte)1); wire.WriteString(opt0); } else { wire.WriteU8((byte)0); };",
            "the first Option field's encode uses opt0",
        );
        assert_source_contains(
            record_source,
            "if (this.Score is { } opt1) { wire.WriteU8((byte)1); wire.WriteF64(opt1); } else { wire.WriteU8((byte)0); };",
            "the second Option field's encode advances to opt1",
        );
        // Decode uses the same ternary form as Option returns.
        assert_source_contains(
            record_source,
            "reader.ReadU8() == 0 ? (string?)null : reader.ReadString()",
            "the string? field decodes through ReadString with the (string?)null cast on the None branch",
        );
        assert_source_contains(
            record_source,
            "reader.ReadU8() == 0 ? (double?)null : reader.ReadF64()",
            "the double? field decodes through ReadF64 with the (double?)null cast on the None branch",
        );
    }

    /// `Option<Vec<T>>` wraps the entire length-prefixed array in the
    /// 1-byte Option tag. The wire codec is: tag byte + (when present)
    /// the normal 4-byte length prefix + elements. The inner Vec still
    /// uses whichever path its element type admits. For primitives,
    /// that's the length-prefixed blittable bulk helper.
    #[test]
    fn emit_option_vec_of_primitive_wraps_blittable_vec_in_option_tag() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_optional_vec",
            vec![(
                "v",
                TypeExpr::Option(Box::new(TypeExpr::Vec(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
            ReturnDef::Value(TypeExpr::Option(Box::new(TypeExpr::Vec(Box::new(
                TypeExpr::Primitive(PrimitiveType::I32),
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static int[]? EchoOptionalVec(int[]? v)",
            "Option<Vec<i32>> renders as int[]?: the `?` nullability applies to the whole array",
        );
        assert_source_contains(
            &src,
            "using var _wire_v = new WireWriter((1 + (v is { } sizeOpt0 ? (4 + sizeOpt0.Length * 4) : 0)));",
            "size sums the 1-byte Option tag with the 4-byte length prefix and the raw element bytes",
        );
        assert_source_contains(
            &src,
            "if (v is { } opt0) { _wire_v.WriteU8((byte)1); _wire_v.WriteBlittableArray(opt0); } else { _wire_v.WriteU8((byte)0); }",
            "encode dispatches to WriteBlittableArray on the unwrapped vec; the fast path is preserved \
             inside the Option wrapping",
        );
        assert_source_contains(
            &src,
            "var reader = new WireReader(_buf); return reader.ReadU8() == 0 ? (int[]?)null : reader.ReadLengthPrefixedBlittableArray<int>();",
            "decode null-casts to int[]? and otherwise reads through the length-prefixed blittable helper",
        );
    }

    /// `Vec<Option<T>>` is the nested composition: the outer vec uses
    /// the encoded-array path (each element is variable-width because
    /// of the Option tag), and each element carries its own 1-byte
    /// present/absent byte plus optional payload. Proves emit can
    /// compose the Option arm inside a Vec arm without the ABI's
    /// placeholder identifiers colliding: the outer vec's loop var
    /// (`item` → `item0`) and the inner Option's unwrapped value
    /// (`v` → `opt0`) occupy separate namespaces.
    #[test]
    fn emit_vec_of_option_composes_per_element_tag_into_encoded_array() {
        let mut contract = empty_contract();
        contract.functions.push(function_with_types(
            "echo_vec_optional_i32",
            vec![(
                "v",
                TypeExpr::Vec(Box::new(TypeExpr::Option(Box::new(TypeExpr::Primitive(
                    PrimitiveType::I32,
                ))))),
            )],
            ReturnDef::Value(TypeExpr::Vec(Box::new(TypeExpr::Option(Box::new(
                TypeExpr::Primitive(PrimitiveType::I32),
            ))))),
        ));

        let src = emit_contract(&contract).combined_source();

        assert_source_contains(
            &src,
            "public static int?[] EchoVecOptionalI32(int?[] v)",
            "Vec<Option<i32>> renders as int?[]: array of Nullable<int>, not a nullable array",
        );
        // Write path: length prefix, foreach with a named iterator,
        // per-element Option encoding.
        assert_source_contains(
            &src,
            "_wire_v.WriteI32(v.Length);",
            "encode writes the i32 length first",
        );
        assert_source_contains(
            &src,
            "foreach (int? item0 in v) { if (item0 is { } opt0) { _wire_v.WriteU8((byte)1); _wire_v.WriteI32(opt0); } else { _wire_v.WriteU8((byte)0); }; }",
            "then loops each element through its own tag + payload",
        );
        // Decode path: ReadEncodedArray with a per-element closure
        // that reads the tag and either returns (int?)null or the
        // primitive.
        assert_source_contains(
            &src,
            "return new WireReader(_buf).ReadEncodedArray<int?>(r0 => r0.ReadU8() == 0 ? (int?)null : r0.ReadI32());",
            "decode walks ReadEncodedArray with a per-element closure that reads the Option tag first, \
             null-casting on the None branch and reading the i32 payload on the Some branch",
        );
    }

    fn empty_class(name: &str) -> ClassDef {
        ClassDef {
            id: ClassId::new(name),
            constructors: vec![],
            methods: vec![],
            streams: vec![],
            doc: None,
            deprecated: None,
        }
    }

    /// A class in the contract emits its own `.cs` file with the
    /// IDisposable wrapper around an opaque `IntPtr` handle. Snake-case
    /// class IDs render as PascalCase C# class and file names.
    #[test]
    fn emit_class_generates_idisposable_wrapper_file() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(empty_class("inventory"));

        let output = emit_contract(&contract);
        let file = output
            .files
            .iter()
            .find(|f| f.file_name == "Inventory.cs")
            .expect("Inventory.cs");

        assert_source_contains(
            &file.source,
            "public sealed class Inventory : IDisposable",
            "class is sealed and implements IDisposable",
        );
        assert_source_contains(
            &file.source,
            "private IntPtr _handle;",
            "the native handle is held as a private IntPtr field",
        );
        assert_source_contains(
            &file.source,
            "private Inventory(IntPtr handle)",
            "handle-adopting ctor is private so an integer literal like \
             `new Inventory(2)` can never resolve here via the implicit \
             literal-to-IntPtr conversion; the chained `: this(...)` form \
             and the static factories live inside the class and can still \
             reach it",
        );
        assert_source_contains(
            &file.source,
            "Interlocked.Exchange(ref _handle, IntPtr.Zero)",
            "Dispose atomically swaps the handle out so a second call is a no-op",
        );
        assert_source_contains(
            &file.source,
            "NativeMethods.InventoryFree(handle);",
            "Dispose forwards the swapped-out handle to the native free symbol",
        );
        assert_source_contains(
            &file.source,
            "GC.SuppressFinalize(this);",
            "Dispose suppresses the finalizer once the handle has been freed",
        );
        assert_source_contains(
            &file.source,
            "~Inventory()",
            "finalizer is the safety net for consumers that forget Dispose",
        );
    }

    /// The `[DllImport]` for the native free function lands in the
    /// shared `NativeMethods` class (which lives in the main wrapper
    /// file), not in the per-class file. The class wrapper just
    /// references it as `NativeMethods.{Class}Free`.
    #[test]
    fn emit_class_registers_dllimport_for_free_in_native_methods() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(empty_class("inventory"));

        let output = emit_contract(&contract);
        let main = output
            .files
            .iter()
            .find(|f| f.file_name == "DemoLib.cs")
            .expect("DemoLib.cs");

        assert_source_contains(
            &main.source,
            r#"[DllImport(LibName, EntryPoint = "boltffi_inventory_free")]"#,
            "free function is registered in NativeMethods using the standard \
             boltffi_{class}_free C symbol",
        );
        assert_source_contains(
            &main.source,
            "internal static extern void InventoryFree(IntPtr handle);",
            "P/Invoke takes the IntPtr directly: void InventoryFree(IntPtr)",
        );
    }

    /// Two classes: each gets its own `.cs` file and its own
    /// owner-prefixed P/Invoke entry. The owner prefix prevents
    /// collisions inside the flat `NativeMethods` class.
    #[test]
    fn emit_multiple_classes_get_disambiguated_native_methods() {
        let mut contract = empty_contract();
        contract.catalog.insert_class(empty_class("inventory"));
        contract.catalog.insert_class(empty_class("counter"));

        let output = emit_contract(&contract);
        assert!(output.files.iter().any(|f| f.file_name == "Inventory.cs"));
        assert!(output.files.iter().any(|f| f.file_name == "Counter.cs"));

        let main_source = &output
            .files
            .iter()
            .find(|f| f.file_name == "DemoLib.cs")
            .expect("DemoLib.cs")
            .source;

        assert_source_contains(
            main_source,
            "InventoryFree(IntPtr handle);",
            "first class's free has its owner prefix",
        );
        assert_source_contains(
            main_source,
            "CounterFree(IntPtr handle);",
            "second class's free has its own owner prefix, no collision \
             with the first inside flat NativeMethods",
        );
    }

    /// Demo crate test: scanning `examples/demo` and emitting C# yields
    /// IDisposable wrappers for each class in the demo (`Inventory`,
    /// `Counter`, etc.) and the matching free DllImports. This pins the
    /// scan-to-emit pipeline end to end against a real fixture, not a
    /// hand-built `ClassDef`.
    #[test]
    fn emit_demo_crate_classes_render_as_idisposable_wrappers() {
        use std::path::PathBuf;

        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let demo_crate_path = repo_root.join("examples").join("demo");
        let mut module = crate::scan::scan_crate(&demo_crate_path, "demo").unwrap();
        let contract = crate::ir::build_contract(&mut module);
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = CSharpEmitter::emit(&contract, &abi, &CSharpOptions::default());

        let inventory = output
            .files
            .iter()
            .find(|f| f.file_name == "Inventory.cs")
            .expect("Inventory.cs from demo crate");
        assert_source_contains(
            &inventory.source,
            "public sealed class Inventory : IDisposable",
            "demo crate's Inventory class lowers to a sealed IDisposable wrapper",
        );
        assert_source_contains(
            &inventory.source,
            "public Inventory()",
            "demo Inventory's `pub fn new()` lifts to a primary C# constructor",
        );
        assert_source_contains(
            &inventory.source,
            "public static Inventory WithCapacity(uint capacity)",
            "demo Inventory's `pub fn with_capacity(u32)` lifts to a static factory",
        );

        let counter = output
            .files
            .iter()
            .find(|f| f.file_name == "Counter.cs")
            .expect("Counter.cs from demo crate");
        assert_source_contains(
            &counter.source,
            "public sealed class Counter : IDisposable",
            "demo crate's Counter class lowers to a sealed IDisposable wrapper",
        );
        assert_source_contains(
            &counter.source,
            "public int Get()",
            "demo Counter's `&self` getter renders as a ClassInstance method",
        );
        assert_source_contains(
            &counter.source,
            "public void Increment()",
            "demo Counter's void `&self` method renders without a return",
        );
        assert_source_contains(
            &counter.source,
            "public Point AsPoint()",
            "demo Counter's blittable-record return is rendered directly",
        );

        let math_utils = output
            .files
            .iter()
            .find(|f| f.file_name == "MathUtils.cs")
            .expect("MathUtils.cs from demo crate");
        assert_source_contains(
            &math_utils.source,
            "public static int Add(int a, int b)",
            "MathUtils::add (no-self) lifts to a `public static` class method",
        );
        assert_source_contains(
            &math_utils.source,
            "public double Round(double value)",
            "MathUtils::round (&self) lifts to a ClassInstance method",
        );

        let main = output
            .files
            .iter()
            .find(|f| f.file_name.ends_with(".cs") && f.source.contains("class NativeMethods"))
            .expect("main wrapper file with NativeMethods");
        assert_source_contains(
            &main.source,
            r#"[DllImport(LibName, EntryPoint = "boltffi_inventory_free")]"#,
            "demo crate emit registers Inventory's free DllImport",
        );
        assert_source_contains(
            &main.source,
            r#"[DllImport(LibName, EntryPoint = "boltffi_counter_free")]"#,
            "demo crate emit registers Counter's free DllImport",
        );
        assert_source_contains(
            &main.source,
            r#"[DllImport(LibName, EntryPoint = "boltffi_inventory_new")]"#,
            "demo crate emit registers Inventory's primary constructor DllImport",
        );
        assert_source_contains(
            &main.source,
            "internal static extern IntPtr InventoryNew();",
            "Inventory primary constructor's native method returns IntPtr",
        );
        assert_source_contains(
            &main.source,
            "internal static extern IntPtr InventoryWithCapacity(uint capacity);",
            "Inventory's named-init constructor takes its explicit param through DllImport",
        );
        assert_source_contains(
            &main.source,
            "internal static extern int CounterGet(IntPtr self);",
            "ClassInstance methods prepend `IntPtr self` to the DllImport signature",
        );
        assert_source_contains(
            &main.source,
            "internal static extern int MathUtilsAdd(int a, int b);",
            "Class static methods do not prepend a receiver to the DllImport",
        );
    }

    /// Pins the shape of the C# bindings for the demo crate's custom
    /// types: `Email` (`#[custom_ffi]`, repr = String), `UtcDateTime`
    /// (`custom_type!`, repr = i64), and `Event` (a `#[data]` record
    /// holding a `DateTime<Utc>` field). Custom types erase to their
    /// repr in the foreign API, but the macro always exposes them as
    /// wire-encoded across the C ABI — even Custom<i64> ships
    /// length-prefixed. The asserts below pin both halves of that
    /// contract: the public C# surface uses repr types, the DllImport
    /// signatures use `byte[] + UIntPtr`, and the wrapper bodies wire-
    /// encode every Custom-typed param (including `Vec<Custom<i64>>`,
    /// where the IR codec disagrees with the substituted ABI shape).
    #[test]
    fn emit_demo_crate_renders_custom_types_through_their_repr() {
        use std::path::PathBuf;

        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let demo_crate_path = repo_root.join("examples").join("demo");
        let mut module = crate::scan::scan_crate(&demo_crate_path, "demo").unwrap();
        let contract = crate::ir::build_contract(&mut module);
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = CSharpEmitter::emit(&contract, &abi, &CSharpOptions::default());

        let main = output
            .files
            .iter()
            .find(|f| f.file_name.ends_with(".cs") && f.source.contains("class NativeMethods"))
            .expect("main wrapper file with NativeMethods");

        // Email (Custom<String>): public surface erases to `string`,
        // but the param wire-encodes (length-prefixed) — sending raw
        // UTF-8 would misalign the Rust decoder.
        assert_source_contains(
            &main.source,
            "public static string EchoEmail(string email)",
            "Email param/return erases to string in the public API",
        );
        assert_source_contains(
            &main.source,
            "_wire_email.WriteString(email);",
            "Email param wire-encodes via WriteString (length-prefixed UTF-8)",
        );
        assert_source_contains(
            &main.source,
            "internal static extern FfiBuf EchoEmail(byte[] email, UIntPtr emailLen);",
            "Custom<String> DllImport takes a byte buffer, not a raw string",
        );

        // UtcDateTime (Custom<i64>): public surface is `long`, but the
        // boundary still wire-encodes to keep the C ABI uniform across
        // Custom types. Return decode goes through the new
        // WireDecodeValue path: `var reader = ...; return reader.ReadI64();`
        assert_source_contains(
            &main.source,
            "public static long EchoDatetime(long dt)",
            "UtcDateTime param/return erases to long in the public API",
        );
        assert_source_contains(
            &main.source,
            "_wire_dt.WriteI64(dt);",
            "UtcDateTime param wire-encodes the i64 even though it's fixed-width",
        );
        assert_source_contains(
            &main.source,
            "var reader = new WireReader(_buf); return reader.ReadI64();",
            "Custom<Primitive> return reads through WireReader, not the direct primitive path",
        );
        assert_source_contains(
            &main.source,
            "internal static extern FfiBuf EchoDatetime(byte[] dt, UIntPtr dtLen);",
            "Custom<i64> DllImport takes a byte buffer, not raw long",
        );

        // Event (record with a DateTime<Utc> field): admission has to
        // see through Custom or the whole record gets dropped. Field
        // type erases to `long` in the rendered class.
        let event_file = output
            .files
            .iter()
            .find(|f| f.file_name == "Event.cs")
            .expect("Event.cs from demo crate (record with a Custom field admits)");
        assert_source_contains(
            &event_file.source,
            "long Timestamp",
            "Event.timestamp field surfaces as long, mirroring the repr of UtcDateTime",
        );
        assert_source_contains(
            &event_file.source,
            "wire.WriteI64(this.Timestamp);",
            "Event encodes the Custom field through its primitive repr writer",
        );

        // Vec<Email>: encoded path. The encoded-array writer iterates
        // emails as `string` (not `Custom<Email>`), so `from_type_expr`
        // never sees a Custom — entry-point normalization prevents
        // the latent todo!.
        assert_source_contains(
            &main.source,
            "foreach (string item0 in emails) { _wire_emails.WriteString(item0); };",
            "Vec<Email> wire-encodes element-wise with string (repr), not Custom",
        );
        assert_source_contains(
            &main.source,
            "return new WireReader(_buf).ReadEncodedArray<string>(r0 => r0.ReadString());",
            "Vec<Email> return reads as encoded array of strings",
        );

        // Vec<UtcDateTime>: the macro's substituted ABI is blittable
        // (`*const i64, usize`) but the codec layout sets `Encoded` for
        // any Vec whose element isn't a bare Primitive. The macro
        // wins — every `Vec<Custom<_>>` ships length-prefixed both
        // ways. Pinning that here so a future change can't regress
        // back to the asymmetric DirectArray-in / encoded-out shape.
        assert_source_contains(
            &main.source,
            "foreach (long item0 in dts) { _wire_dts.WriteI64(item0); };",
            "Vec<UtcDateTime> wire-encodes element-wise as long, not raw long[]",
        );
        assert_source_contains(
            &main.source,
            "return new WireReader(_buf).ReadEncodedArray<long>(r0 => r0.ReadI64());",
            "Vec<UtcDateTime> return reads as encoded array, not raw blittable",
        );
        assert_source_contains(
            &main.source,
            "internal static extern FfiBuf EchoDatetimes(byte[] dts, UIntPtr dtsLen);",
            "Vec<Custom<i64>> DllImport takes a byte buffer regardless of repr blittability",
        );
    }

    /// End-to-end pin for throwing methods (issue #146): scanning the
    /// demo crate emits the runtime `BoltException` class, the typed
    /// exception companions for each `#[error]` enum and record, and
    /// wrapper bodies that branch on the wire tag and throw the right
    /// shape. Pins all four Err transports — string, primitive, typed
    /// enum, typed record — so silent regressions in any of them fail
    /// `cargo test` without needing the C# acceptance harness.
    #[test]
    fn emit_demo_crate_throwing_methods_render_typed_exceptions() {
        use std::path::PathBuf;

        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let demo_crate_path = repo_root.join("examples").join("demo");
        let mut module = crate::scan::scan_crate(&demo_crate_path, "demo").unwrap();
        let contract = crate::ir::build_contract(&mut module);
        let abi = IrLowerer::new(&contract).to_abi_contract();
        let output = CSharpEmitter::emit(&contract, &abi, &CSharpOptions::default());

        // BoltException is bundled into the main wrapper file (alongside
        // FfiBuf / WireReader). Anything emitting the runtime class has
        // to gate on `module.needs_bolt_exception()`; the demo crate has
        // string-Err results, so the predicate fires and the class lands.
        let main = output
            .files
            .iter()
            .find(|f| f.file_name.ends_with(".cs") && f.source.contains("class NativeMethods"))
            .expect("main wrapper file with NativeMethods");
        assert_source_contains(
            &main.source,
            "public sealed class BoltException : Exception",
            "BoltException is emitted at module scope when any Err transport is non-typed",
        );

        // String-Err function: the wrapper reads the tag and throws the
        // runtime `BoltException` carrying the wire-decoded message.
        // Mirrors the demo's `safe_divide(i32, i32) -> Result<i32, String>`.
        assert_source_contains(
            &main.source,
            "if (reader.ReadU8() != 0) throw new BoltException(reader.ReadString());",
            "String-Err result wrappers throw BoltException with the wire-decoded message",
        );

        // Typed enum Err: the demo's `MathError` is `#[error]`, so the
        // generated `MathError.cs` carries a companion `MathErrorException`
        // with an `Error` property bound to the wrapped variant.
        let math_error_file = output
            .files
            .iter()
            .find(|f| f.file_name == "MathError.cs")
            .expect("MathError.cs from demo crate");
        assert_source_contains(
            &math_error_file.source,
            "public sealed class MathErrorException : Exception",
            "#[error] enums emit a typed exception alongside the enum itself",
        );
        assert_source_contains(
            &math_error_file.source,
            "public MathError Error { get; }",
            "Typed exceptions expose the wrapped value through an Error property",
        );

        // Throwing function whose Err is the typed enum: the wrapper
        // body decodes the wire-encoded enum value and throws the typed
        // exception. Pins `throw new MathErrorException(MathErrorWire.Decode(reader))`
        // — the canonical shape produced by the typed-exception path.
        assert_source_contains(
            &main.source,
            "throw new MathErrorException(MathErrorWire.Decode(reader));",
            "Typed enum Err wrappers throw <Name>Exception(<Name>Wire.Decode(reader))",
        );

        // Typed record Err: same shape but the wrapped value is a
        // record. AppError has a `Message: string` field, so the
        // exception's base `Exception.Message` forwards from
        // `error.Message` rather than the verbose record `ToString()`.
        let app_error_file = output
            .files
            .iter()
            .find(|f| f.file_name == "AppError.cs")
            .expect("AppError.cs from demo crate");
        assert_source_contains(
            &app_error_file.source,
            "public sealed class AppErrorException : Exception",
            "#[error] records emit a typed exception alongside the record itself",
        );
        assert_source_contains(
            &app_error_file.source,
            "public AppErrorException(AppError error) : base(error.Message)",
            "AppError forwards its Message field through to the base Exception ctor \
             so callers see a focused message instead of the auto record ToString",
        );
        assert_source_contains(
            &main.source,
            "throw new AppErrorException(AppError.Decode(reader));",
            "Typed record Err wrappers decode the record and throw <Name>Exception",
        );

        // Class method that returns Result: pins the throwing-body
        // shape inside a class wrapper. `Counter::try_get_positive`
        // returns `Result<i32, String>`, so the body reads the tag,
        // throws `BoltException` on Err, and returns the decoded int
        // on Ok.
        let counter_file = output
            .files
            .iter()
            .find(|f| f.file_name == "Counter.cs")
            .expect("Counter.cs from demo crate");
        assert_source_contains(
            &counter_file.source,
            "public int TryGetPositive()",
            "Counter::try_get_positive lifts to a public throwing method returning int",
        );
        assert_source_contains(
            &counter_file.source,
            "if (reader.ReadU8() != 0) throw new BoltException(reader.ReadString());",
            "Class throwing methods reuse the same WireDecodeResult body shape \
             as top-level functions",
        );
    }
}
