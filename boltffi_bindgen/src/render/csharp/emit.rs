//! Orchestrates the lowerer and templates to produce the final `.cs` output.
//!
//! Also hosts the C#-syntax helpers that translate ABI ops
//! ([`ReadOp`], [`WriteOp`], [`SizeExpr`], [`ValueExpr`]) into source
//! snippets. The lowerer calls these helpers to pre-render the wire
//! expressions that end up in [`CSharpRecordField`] and
//! [`CSharpWireWriter`]. Keeping the syntax formatting here (and the
//! "which ops apply to which field" logic in [`lower`](super::lower))
//! mirrors the Java backend split.

use std::collections::HashSet;

use askama::Template as _;

use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ops::{ReadOp, ReadSeq, SizeExpr, ValueExpr, WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};
use crate::ir::{AbiContract, FfiContract};

use super::{
    CSharpOptions, NamingConvention,
    lower::CSharpLowerer,
    plan::{CSharpEnumKind, CSharpRecord},
    templates::{
        EnumCStyleTemplate, EnumDataTemplate, FunctionsTemplate, NativeTemplate, PreambleTemplate,
        RecordTemplate,
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
    /// The top-level class name (used for the main file, e.g., `"MyApp"`).
    pub class_name: String,
    /// The C# namespace.
    pub namespace: String,
}

impl CSharpOutput {
    /// Concatenation of every file's source text. Convenience for tests
    /// and spot-checks that only care about "did this snippet appear
    /// anywhere in the generated code?"
    #[cfg(test)]
    pub fn combined_source(&self) -> String {
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

        CSharpOutput {
            files,
            class_name: module.class_name,
            namespace: module.namespace,
        }
    }
}

// ---------------------------------------------------------------------------
// Render helpers: ABI ops -> C# syntax snippets.
//
// These are the C# counterparts of the functions in render/java/emit.rs.
// Each takes a [`ReadSeq`] / [`WriteSeq`] / [`SizeExpr`] / [`ValueExpr`]
// node from the ABI and produces the C# source that implements it.
//
// Scope: supports the subset of ops we need for records with primitive,
// string, and nested-record fields. Vec / Option / Enum / Builtin / Custom
// etc. will be added in follow-up PRs — today they panic so the gap is
// surfaced loudly rather than silently producing broken output.
// ---------------------------------------------------------------------------

/// Render a [`ValueExpr`] as a C# value access path.
///
/// [`Instance`](ValueExpr::Instance) becomes `this.` (trailing dot joined
/// by the field walk). A [`Field`](ValueExpr::Field) chain walks outward
/// producing e.g. `this.Origin.X`. Parameter references keep their
/// camelCase name; field references convert to PascalCase property names
/// to match the record struct definition.
pub fn render_value(expr: &ValueExpr) -> String {
    match expr {
        ValueExpr::Instance => "this".to_string(),
        ValueExpr::Var(name) => name.clone(),
        ValueExpr::Named(name) => NamingConvention::field_name(name),
        ValueExpr::Field(parent, field) => {
            let parent_str = render_value(parent);
            let field_str = NamingConvention::property_name(field.as_str());
            format!("{}.{}", parent_str, field_str)
        }
    }
}

pub fn primitive_read_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "ReadBool",
        PrimitiveType::I8 => "ReadI8",
        PrimitiveType::U8 => "ReadU8",
        PrimitiveType::I16 => "ReadI16",
        PrimitiveType::U16 => "ReadU16",
        PrimitiveType::I32 => "ReadI32",
        PrimitiveType::U32 => "ReadU32",
        PrimitiveType::I64 => "ReadI64",
        PrimitiveType::U64 => "ReadU64",
        PrimitiveType::ISize => "ReadNInt",
        PrimitiveType::USize => "ReadNUInt",
        PrimitiveType::F32 => "ReadF32",
        PrimitiveType::F64 => "ReadF64",
    }
}

pub fn primitive_write_method(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::Bool => "WriteBool",
        PrimitiveType::I8 => "WriteI8",
        PrimitiveType::U8 => "WriteU8",
        PrimitiveType::I16 => "WriteI16",
        PrimitiveType::U16 => "WriteU16",
        PrimitiveType::I32 => "WriteI32",
        PrimitiveType::U32 => "WriteU32",
        PrimitiveType::I64 => "WriteI64",
        PrimitiveType::U64 => "WriteU64",
        PrimitiveType::ISize => "WriteNInt",
        PrimitiveType::USize => "WriteNUInt",
        PrimitiveType::F32 => "WriteF32",
        PrimitiveType::F64 => "WriteF64",
    }
}

/// The WireReader method invocation for a top-level blittable primitive
/// `Vec<T>` return, without the receiver. Top-level vec returns carry no
/// length prefix: the count comes from `FfiBuf.len`. Bool, isize, and
/// usize have dedicated methods because C# `bool` is not `unmanaged`-cast-
/// safe against the 1-byte wire form, and the wire form of isize/usize is
/// a fixed 8 bytes while C# `nint`/`nuint` are pointer-sized. Every other
/// primitive flows through a generic `ReadBlittableArray<T>()` that bulk-
/// casts via `MemoryMarshal`.
pub fn primitive_vec_reader_call(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "ReadBoolArray()".to_string(),
        PrimitiveType::ISize => "ReadNIntArray()".to_string(),
        PrimitiveType::USize => "ReadNUIntArray()".to_string(),
        other => format!(
            "ReadBlittableArray<{}>()",
            super::mappings::csharp_type(other)
        ),
    }
}

/// The WireReader method invocation for a nested blittable primitive
/// `Vec<T>`, without the receiver. Nested reads carry a 4-byte length
/// prefix because the outer encoded container needs to know how far to
/// advance the cursor before decoding the next element.
fn primitive_vec_nested_reader_call(primitive: PrimitiveType) -> String {
    match primitive {
        PrimitiveType::Bool => "ReadLengthPrefixedBoolArray()".to_string(),
        PrimitiveType::ISize => "ReadLengthPrefixedNIntArray()".to_string(),
        PrimitiveType::USize => "ReadLengthPrefixedNUIntArray()".to_string(),
        other => format!(
            "ReadLengthPrefixedBlittableArray<{}>()",
            super::mappings::csharp_type(other)
        ),
    }
}

/// The WireWriter method invocation for a blittable primitive `Vec<T>`,
/// formatted as `"{method}({value})"` without the receiver. The writer
/// always prepends a 4-byte length prefix, which serves the nested
/// position (inside an encoded outer vec) and the future record-field
/// position. Top-level vec params go through `DirectArray` and never
/// touch this code path.
pub fn primitive_vec_writer_call(primitive: PrimitiveType, value: &str) -> String {
    match primitive {
        PrimitiveType::Bool => format!("WriteBoolArray({value})"),
        PrimitiveType::ISize => format!("WriteNIntArray({value})"),
        PrimitiveType::USize => format!("WriteNUIntArray({value})"),
        _ => format!("WriteBlittableArray({value})"),
    }
}

/// Per-function rendering scratchpad. Hands out unique loop-variable and
/// closure-argument names so nested `Vec<Vec<_>>` expressions don't
/// shadow each other. One instance per top-level emit call; counters are
/// consumed in the order read / write / size helpers encounter nested
/// vecs, which is stable across renders of the same seq.
#[derive(Default)]
pub struct CSharpEmitContext {
    write_loop_index: usize,
    read_closure_index: usize,
    size_loop_index: usize,
}

impl CSharpEmitContext {
    fn next_write_loop_var(&mut self) -> String {
        let i = self.write_loop_index;
        self.write_loop_index += 1;
        format!("item{}", i)
    }

    fn next_read_closure_var(&mut self) -> String {
        let i = self.read_closure_index;
        self.read_closure_index += 1;
        format!("r{}", i)
    }

    fn next_size_loop_var(&mut self) -> String {
        let i = self.size_loop_index;
        self.size_loop_index += 1;
        format!("sizeItem{}", i)
    }
}

/// Replace every word-boundary occurrence of `identifier` in `expression`
/// with `replacement`. A token inside another identifier (e.g. `item`
/// inside `item0`) is left alone. Used to rename the ABI-provided loop
/// variable `"item"` and closure receiver `"reader"` to per-nesting-level
/// unique names without running a real parser.
fn replace_identifier_occurrences(expression: &str, identifier: &str, replacement: &str) -> String {
    if identifier.is_empty() {
        return expression.to_string();
    }
    let mut result = String::with_capacity(expression.len());
    let mut cursor = 0;
    while let Some(rel) = expression[cursor..].find(identifier) {
        let start = cursor + rel;
        let end = start + identifier.len();
        let prev = expression[..start].chars().next_back();
        let next = expression[end..].chars().next();
        let prev_is_id = prev.map(is_identifier_char).unwrap_or(false);
        let next_is_id = next.map(is_identifier_char).unwrap_or(false);
        if prev_is_id || next_is_id {
            result.push_str(&expression[cursor..end]);
        } else {
            result.push_str(&expression[cursor..start]);
            result.push_str(replacement);
        }
        cursor = end;
    }
    result.push_str(&expression[cursor..]);
    result
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Render the `reader_call` portion (without the leading `new WireReader
/// (_buf).` receiver) for a top-level `Vec<inner>` return. Primitive
/// element vecs use the no-prefix fast path keyed off `FfiBuf.len`;
/// encoded element vecs (strings, nested vecs) wrap `ReadEncodedArray<T>`
/// around a closure that decodes one element.
pub fn vec_return_reader_call(inner: &TypeExpr, element_seq: &ReadSeq) -> String {
    match inner {
        TypeExpr::Primitive(p) => primitive_vec_reader_call(*p),
        _ => {
            let mut ctx = CSharpEmitContext::default();
            let inner_reader = emit_reader_read_with_context(element_seq, None, &mut ctx);
            let closure_var = ctx.next_read_closure_var();
            let remapped = replace_identifier_occurrences(&inner_reader, "reader", &closure_var);
            let element_type = csharp_type_for_inner(inner);
            format!(
                "ReadEncodedArray<{}>({} => {})",
                element_type, closure_var, remapped
            )
        }
    }
}

/// C# type literal for a `Vec<_>` element. Used when stamping the generic
/// parameter on `ReadEncodedArray<T>` at emit time. Records and enums
/// render as their PascalCase class name. Vec elements are only typed at
/// the function-level wrapper, never inside a nested enum body, so the
/// shadowing machinery that qualifies record decodes inside data-enum
/// variants does not apply here.
fn csharp_type_for_inner(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Primitive(p) => super::mappings::csharp_type(*p).to_string(),
        TypeExpr::String => "string".to_string(),
        TypeExpr::Record(id) => NamingConvention::class_name(id.as_str()),
        TypeExpr::Enum(id) => NamingConvention::class_name(id.as_str()),
        TypeExpr::Vec(inner) => format!("{}[]", csharp_type_for_inner(inner)),
        other => todo!(
            "csharp_type_for_inner: Vec element {:?} is not yet supported by the C# backend",
            other
        ),
    }
}

/// Names shadowed by a nested scope in the rendering site. Passed to
/// [`emit_reader_read`] when emitting decode expressions inside a data
/// enum body, where nested `sealed record Foo() : E` variants shadow
/// module-level types of the same name. Any class reference whose name
/// appears in `shadowed` gets qualified as `{namespace}.{ClassName}` so
/// it resolves past the nested variant.
pub struct ShadowScope<'a> {
    pub shadowed: &'a HashSet<String>,
    pub namespace: &'a str,
}

/// Render the first op of a [`ReadSeq`] as a decode expression.
///
/// `scope` is `None` when rendering in a context where no name shadowing
/// can happen (record bodies, top-level function returns) and `Some` when
/// rendering inside a data enum body whose nested variants may shadow
/// module-level types.
///
/// Today each [`ReadSeq`] we handle has exactly one top-level op — either
/// a primitive, a string, or a nested record/enum. Container ops (Option,
/// Vec, Result) will land in follow-up PRs.
pub fn emit_reader_read(seq: &ReadSeq, scope: Option<&ShadowScope>) -> String {
    let mut ctx = CSharpEmitContext::default();
    emit_reader_read_with_context(seq, scope, &mut ctx)
}

fn emit_reader_read_with_context(
    seq: &ReadSeq,
    scope: Option<&ShadowScope>,
    ctx: &mut CSharpEmitContext,
) -> String {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => {
            format!("reader.{}()", primitive_read_method(*primitive))
        }
        ReadOp::String { .. } => "reader.ReadString()".to_string(),
        ReadOp::Bytes { .. } => "reader.ReadBytes()".to_string(),
        ReadOp::Record { id, .. } => {
            let class_name = NamingConvention::class_name(id.as_str());
            format!("{}.Decode(reader)", qualify_if_shadowed(&class_name, scope))
        }
        ReadOp::Enum {
            id,
            layout: EnumLayout::CStyle { .. },
            ..
        } => {
            // The generated helper is `{Name}Wire`, not `{Name}`, so the
            // `Wire` suffix is already unambiguous against variant names
            // that match `{Name}` alone; no shadowing fix needed here.
            format!(
                "{}Wire.Decode(reader)",
                NamingConvention::class_name(id.as_str())
            )
        }
        ReadOp::Enum {
            id,
            layout: EnumLayout::Data { .. },
            ..
        } => {
            let class_name = NamingConvention::class_name(id.as_str());
            format!("{}.Decode(reader)", qualify_if_shadowed(&class_name, scope))
        }
        ReadOp::Vec {
            element_type: TypeExpr::Primitive(p),
            layout: VecLayout::Blittable { .. },
            ..
        } => {
            // A Vec op reached through emit_reader_read is always nested
            // (top-level vec returns go through vec_return_reader_call).
            // Nested position means the wire shape is length-prefixed.
            format!("reader.{}", primitive_vec_nested_reader_call(*p))
        }
        ReadOp::Vec {
            element_type,
            layout: VecLayout::Blittable { .. },
            ..
        } => {
            // Reached when a record field or enum-variant field carries
            // a `Vec<BlittableRecord>`. Nested position so the wire shape
            // is length-prefixed, same as nested primitive vecs. The C#
            // struct generated for a blittable record is `unmanaged`, so
            // it satisfies the generic helper's constraint.
            format!(
                "reader.ReadLengthPrefixedBlittableArray<{}>()",
                csharp_type_for_inner(element_type)
            )
        }
        ReadOp::Vec {
            element_type,
            element,
            layout: VecLayout::Encoded,
            ..
        } => {
            let inner_reader = emit_reader_read_with_context(element, scope, ctx);
            let closure_var = ctx.next_read_closure_var();
            let remapped = replace_identifier_occurrences(&inner_reader, "reader", &closure_var);
            let element_type_str = csharp_type_for_inner(element_type);
            format!(
                "reader.ReadEncodedArray<{}>({} => {})",
                element_type_str, closure_var, remapped
            )
        }
        other => todo!(
            "C# backend has not yet implemented read support for {:?}",
            other
        ),
    }
}

/// If `class_name` is shadowed by an enclosing scope, return the
/// fully-qualified `"global::{namespace}.{class_name}"`; otherwise
/// return `class_name` bare. The `global::` prefix skips both the
/// nested-type shadow *and* any same-named class in the current
/// namespace (the generated top-level wrapper class shares its name
/// with the namespace itself, which would otherwise catch a bare
/// `{namespace}.{class_name}` lookup).
fn qualify_if_shadowed(class_name: &str, scope: Option<&ShadowScope>) -> String {
    match scope {
        Some(s) if s.shadowed.contains(class_name) => {
            format!("global::{}.{}", s.namespace, class_name)
        }
        _ => class_name.to_string(),
    }
}

/// Render the first op of a [`WriteSeq`] as a statement that writes its
/// value into the `WireWriter` named by `writer_name`.
pub fn emit_write_expr(seq: &WriteSeq, writer_name: &str) -> String {
    let mut ctx = CSharpEmitContext::default();
    emit_write_expr_with_context(seq, writer_name, &mut ctx)
}

fn emit_write_expr_with_context(
    seq: &WriteSeq,
    writer_name: &str,
    ctx: &mut CSharpEmitContext,
) -> String {
    let op = seq.ops.first().expect("write ops");
    match op {
        WriteOp::Primitive { primitive, value } => {
            format!(
                "{}.{}({})",
                writer_name,
                primitive_write_method(*primitive),
                render_value(value)
            )
        }
        WriteOp::String { value } => {
            format!("{}.WriteString({})", writer_name, render_value(value))
        }
        WriteOp::Bytes { value } => {
            format!("{}.WriteBytes({})", writer_name, render_value(value))
        }
        WriteOp::Record { value, .. } => {
            format!("{}.WireEncodeTo({})", render_value(value), writer_name)
        }
        WriteOp::Enum {
            value,
            layout: EnumLayout::CStyle { .. } | EnumLayout::Data { .. },
            ..
        } => format!("{}.WireEncodeTo({})", render_value(value), writer_name),
        WriteOp::Vec {
            value,
            element_type: TypeExpr::Primitive(p),
            layout: VecLayout::Blittable { .. },
            ..
        } => format!(
            "{}.{}",
            writer_name,
            primitive_vec_writer_call(*p, &render_value(value))
        ),
        WriteOp::Vec {
            value,
            layout: VecLayout::Blittable { .. },
            ..
        } => {
            // Reached when a record field or enum-variant field carries
            // a `Vec<BlittableRecord>`. `WriteBlittableArray<T>` already
            // emits the 4-byte length prefix followed by the raw element
            // bytes, which is the nested wire shape Rust expects. The
            // unmanaged constraint is satisfied by the generated struct.
            format!(
                "{}.WriteBlittableArray({})",
                writer_name,
                render_value(value),
            )
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout: VecLayout::Encoded,
        } => {
            let inner_write = emit_write_expr_with_context(element, writer_name, ctx);
            let loop_var = ctx.next_write_loop_var();
            let remapped = replace_identifier_occurrences(&inner_write, "item", &loop_var);
            let iter_type = csharp_type_for_inner(element_type);
            format!(
                "{}.WriteI32({}.Length); foreach ({} {} in {}) {{ {}; }}",
                writer_name,
                render_value(value),
                iter_type,
                loop_var,
                render_value(value),
                remapped,
            )
        }
        other => todo!(
            "C# backend has not yet implemented write support for {:?}",
            other
        ),
    }
}

/// Render a [`SizeExpr`] as a C# expression that evaluates to the
/// wire-encoded byte size.
///
/// The IR's convention for variable-length types is a
/// `Sum([Fixed(4), StringLen(v)])` or `Sum([Fixed(4), BytesLen(v)])`:
/// the outer `Sum` already accounts for the 4-byte length prefix, so
/// `StringLen` and `BytesLen` must render as just the payload byte
/// count. Doubling up (e.g. rendering `StringLen` as `4 + byte_count`)
/// would over-count by 4 bytes per string.
pub fn emit_size_expr(size: &SizeExpr) -> String {
    let mut ctx = CSharpEmitContext::default();
    emit_size_expr_with_context(size, &mut ctx)
}

fn emit_size_expr_with_context(size: &SizeExpr, ctx: &mut CSharpEmitContext) -> String {
    match size {
        SizeExpr::Fixed(value) => value.to_string(),
        SizeExpr::StringLen(value) => {
            format!("Encoding.UTF8.GetByteCount({})", render_value(value))
        }
        SizeExpr::BytesLen(value) => format!("{}.Length", render_value(value)),
        SizeExpr::WireSize { value, .. } => {
            format!("{}.WireEncodedSize()", render_value(value))
        }
        SizeExpr::Sum(parts) => {
            let rendered = parts
                .iter()
                .map(|p| emit_size_expr_with_context(p, ctx))
                .collect::<Vec<_>>()
                .join(" + ");
            format!("({})", rendered)
        }
        SizeExpr::VecSize {
            value,
            layout: VecLayout::Blittable { element_size },
            ..
        } => format!("(4 + {}.Length * {})", render_value(value), element_size),
        SizeExpr::VecSize {
            value,
            inner,
            layout: VecLayout::Encoded,
        } => {
            let inner_expr = emit_size_expr_with_context(inner, ctx);
            let loop_var = ctx.next_size_loop_var();
            let remapped = replace_identifier_occurrences(&inner_expr, "item", &loop_var);
            format!(
                "WireWriter.EncodedArraySize({}, {} => {})",
                render_value(value),
                loop_var,
                remapped,
            )
        }
        other => todo!(
            "C# backend has not yet implemented size expression support for {:?}",
            other
        ),
    }
}

// ---------------------------------------------------------------------------
// Ignore unused-import warnings for CSharpRecord in emit.rs while the
// record template type is defined in templates.rs.
// ---------------------------------------------------------------------------
#[allow(dead_code)]
const _: fn(&CSharpRecord) = |_| {};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Lowerer as IrLowerer;
    use crate::ir::codec::EnumLayout;
    use crate::ir::contract::{FfiContract, PackageInfo};
    use crate::ir::definitions::{
        CStyleVariant, DataVariant, EnumDef, EnumRepr, FieldDef, FunctionDef, MethodDef, ParamDef,
        ParamPassing, Receiver, RecordDef, ReturnDef, VariantPayload,
    };
    use crate::ir::ids::{EnumId, FieldName, FunctionId, MethodId, ParamName, RecordId};
    use crate::ir::ops::{OffsetExpr, SizeExpr, ValueExpr, WireShape};
    use crate::ir::types::{PrimitiveType, TypeExpr};
    use boltffi_ffi_rules::callable::ExecutionKind;
    use boltffi_ffi_rules::transport::EnumTagStrategy;

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

        assert_eq!(output.namespace, "DemoLib");
        assert_eq!(output.class_name, "DemoLib");
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
    /// needed when a module actually traffics in wire-encoded returns —
    /// primitive-only output should not carry the extra helpers.
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

    /// Regression: the IR wraps variable-length sizes in
    /// `Sum([Fixed(4), StringLen(v)])`; `StringLen` must render as the
    /// payload byte count alone, not `4 + byte_count`, otherwise a string
    /// field's wire size is over-counted by 4 bytes.
    #[test]
    fn emit_size_expr_for_string_len_renders_payload_only() {
        let size = SizeExpr::Sum(vec![
            SizeExpr::Fixed(4),
            SizeExpr::StringLen(ValueExpr::Named(
                ParamName::new("name").as_str().to_string(),
            )),
        ]);
        assert_eq!(
            emit_size_expr(&size),
            "(4 + Encoding.UTF8.GetByteCount(name))"
        );
    }

    /// A `ReadOp::Enum` with a C-style layout routes to the generated
    /// `{Name}Wire.Decode(reader)` helper rather than inlining the cast.
    /// Keeps the tag-width choice in one place — the enum's wire helper —
    /// so field-level emit code stays oblivious to i32-vs-i16 changes.
    #[test]
    fn emit_reader_read_c_style_enum_calls_wire_helper() {
        let seq = ReadSeq {
            size: SizeExpr::Fixed(4),
            ops: vec![ReadOp::Enum {
                id: EnumId::new("status"),
                offset: OffsetExpr::Fixed(0),
                layout: EnumLayout::CStyle {
                    tag_type: PrimitiveType::I32,
                    tag_strategy: EnumTagStrategy::OrdinalIndex,
                    is_error: false,
                },
            }],
            shape: WireShape::Value,
        };

        assert_eq!(emit_reader_read(&seq, None), "StatusWire.Decode(reader)");
    }

    /// A record decode inside a data-enum body must name-qualify to the
    /// module namespace when the outer type collides with a sibling
    /// variant name. Without the fix, `Point.Decode(reader)` would
    /// resolve to the nested `sealed record Point()` (no Decode method),
    /// breaking compilation.
    #[test]
    fn emit_reader_read_qualifies_record_when_shadowed_by_sibling_variant() {
        let seq = ReadSeq {
            size: SizeExpr::Fixed(16),
            ops: vec![ReadOp::Record {
                id: RecordId::new("point"),
                offset: OffsetExpr::Fixed(0),
                fields: vec![],
            }],
            shape: WireShape::Value,
        };
        let shadowed: HashSet<String> = ["Point".to_string()].into_iter().collect();
        let scope = ShadowScope {
            shadowed: &shadowed,
            namespace: "Demo",
        };

        assert_eq!(
            emit_reader_read(&seq, Some(&scope)),
            "global::Demo.Point.Decode(reader)"
        );
    }

    /// The shadowing pass is inert when the referenced class name is not
    /// in the shadow set — record decodes stay unqualified so we don't
    /// pollute call sites that don't need the namespace prefix.
    #[test]
    fn emit_reader_read_leaves_record_unqualified_when_not_shadowed() {
        let seq = ReadSeq {
            size: SizeExpr::Fixed(16),
            ops: vec![ReadOp::Record {
                id: RecordId::new("point"),
                offset: OffsetExpr::Fixed(0),
                fields: vec![],
            }],
            shape: WireShape::Value,
        };
        let shadowed: HashSet<String> = ["Circle".to_string()].into_iter().collect();
        let scope = ShadowScope {
            shadowed: &shadowed,
            namespace: "Demo",
        };

        assert_eq!(emit_reader_read(&seq, Some(&scope)), "Point.Decode(reader)");
    }

    /// A `WriteOp::Enum` with a C-style layout emits the same call shape
    /// as a record field — `{value}.WireEncodeTo(wire)`. The extension
    /// method on the generated `{Name}Wire` class lets the enum slot into
    /// that uniform shape at no runtime cost.
    #[test]
    fn emit_write_expr_c_style_enum_field_matches_record_call_shape() {
        let value = ValueExpr::Field(Box::new(ValueExpr::Instance), FieldName::new("status"));
        let seq = WriteSeq {
            size: SizeExpr::Fixed(4),
            ops: vec![WriteOp::Enum {
                id: EnumId::new("status"),
                value,
                layout: EnumLayout::CStyle {
                    tag_type: PrimitiveType::I32,
                    tag_strategy: EnumTagStrategy::OrdinalIndex,
                    is_error: false,
                },
            }],
            shape: WireShape::Value,
        };

        assert_eq!(
            emit_write_expr(&seq, "wire"),
            "this.Status.WireEncodeTo(wire)"
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
            "readonly record struct declaration — value type with generated equality",
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
            "single-line delegating body — no WireWriter, no FfiBuf",
        );
        assert_source_contains(
            &src,
            "internal static extern Point EchoPoint(Point p);",
            "DllImport takes and returns the struct directly",
        );
        assert_source_lacks(
            &src,
            "WireWriter(p.WireEncodedSize())",
            "no WireWriter setup for a blittable param — that would defeat the zero-copy win",
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

    /// A non-blittable record (one with a string field) must NOT carry
    /// `[StructLayout(Sequential)]` — its memory layout doesn't need to
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
    /// wire codec just like a non-blittable record — the public wrapper
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
            "the wrapper body to drive the data enum's own WireEncodeTo — same call shape as records",
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
    /// P/Invoke with zero ceremony — the DllImport signature names the
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
            "the wrapper body to pass the enum through unchanged — no cast required",
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
            "_wire_v.WriteI32(v.Length); foreach (string item0 in v) { _wire_v.WriteString(item0); }",
            "the encode body writes the 4-byte count then loops WriteString over each element",
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
    /// unique across nesting (`item0` for the inner write, `item1` for the
    /// outer) so inner references don't shadow the outer.
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
            "_wire_v.WriteI32(v.Length); foreach (int[] item0 in v) { _wire_v.WriteBlittableArray(item0); }",
            "the outer write emits the count then loops WriteBlittableArray (which writes its own length prefix) over each inner array",
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
    /// (`item0` innermost, `item1` outer).
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
            "return new WireReader(_buf).ReadEncodedArray<string[]>(r1 => r1.ReadEncodedArray<string>(r0 => r0.ReadString()));",
            "the return decodes through two nested ReadEncodedArray closures with distinct receiver names",
        );
        assert_source_contains(
            &src,
            "_wire_v.WriteI32(v.Length); foreach (string[] item1 in v) { _wire_v.WriteI32(item1.Length); foreach (string item0 in item1) { _wire_v.WriteString(item0); }; }",
            "the encode body nests two foreach loops with distinct loop variables",
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
            "ByGroups _v => WireWriter.EncodedArraySize(_v.Groups, sizeItem1 => WireWriter.EncodedArraySize(sizeItem1, sizeItem0 => (4 + Encoding.UTF8.GetByteCount(sizeItem0))))",
            "the size expression to prefix only the outer field access and keep distinct nested lambda variables",
        );
        assert_source_contains(
            enum_src,
            "wire.WriteI32(_v.Groups.Length); foreach (string[] item1 in _v.Groups) { wire.WriteI32(item1.Length); foreach (string item0 in item1) { wire.WriteString(item0); }; }",
            "the encode body to prefix only the outer field access and keep the nested foreach bindings untouched",
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
            "_wire_values.WriteI32(values.Length); foreach (Status item0 in values) { item0.WireEncodeTo(_wire_values); }",
            "the encode body writes the 4-byte count then loops WireEncodeTo over each enum value",
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
            "_wire_values.WriteI32(values.Length); foreach (Shape item0 in values) { item0.WireEncodeTo(_wire_values); }",
            "the encode body writes the count and loops the data enum's WireEncodeTo over each element",
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
            "_wire_people.WriteI32(people.Length); foreach (Person item0 in people) { item0.WireEncodeTo(_wire_people); }",
            "the encode body writes the count and loops the record's WireEncodeTo over each element",
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
}
