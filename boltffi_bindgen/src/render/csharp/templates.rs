//! Templates are the `View` of the C# backend: each one binds to a
//! `render_csharp/*.txt` Askama file and renders a plan node as C#
//! source.
//!
//! Templates do no decision-making themselves; all branching and
//! conditional logic lives upstream in `lower`. Templates only
//! interpolate values that the plan and its `ast` primitives carry.
//!
//! Snapshot tests pin the rendered shape against curated plan
//! fixtures.
//!
//! Templates are instantiated and rendered by the `emit` module.

use askama::Template;

use super::ast::{CSharpComment, CSharpNamespace};
use super::plan::{
    CSharpClassPlan, CSharpConstructorKind, CSharpEnumPlan, CSharpFieldPlan, CSharpModulePlan,
    CSharpParamKind, CSharpRecordPlan, CSharpReturnKind,
};

/// Renders a `<summary>` doc block at `indent`, ending with a
/// trailing newline so the declaration that follows lands on the next
/// line. Returns the empty string when the comment is absent so the
/// declaration emits flush against the previous line; templates call
/// this inline with the declaration:
///
/// ```askama
/// {{ self::summary_doc_block(record.summary_doc, "    ") }}    public readonly record struct ...
/// ```
pub fn summary_doc_block(doc: &Option<CSharpComment>, indent: &str) -> String {
    let Some(comment) = doc.as_ref() else {
        return String::new();
    };
    let mut out = String::new();
    push_doc_line(&mut out, indent, "<summary>");
    for line in comment.lines() {
        push_text_line(&mut out, indent, line);
    }
    push_doc_line(&mut out, indent, "</summary>");
    out
}

/// Renders one `<param name="...">` block at `indent` for each field
/// that carries a doc comment. Used on positional records and
/// data-enum variants where individual fields have no separate
/// declaration to attach a `<summary>` to.
pub fn param_doc_block(fields: &[CSharpFieldPlan], indent: &str) -> String {
    let mut out = String::new();
    for field in fields {
        let Some(comment) = field.summary_doc.as_ref() else {
            continue;
        };
        let mut iter = comment.lines();
        let first = iter.next().unwrap_or("");
        let rest: Vec<&str> = iter.collect();
        if rest.is_empty() {
            push_doc_line(
                &mut out,
                indent,
                &format!("<param name=\"{}\">{}</param>", field.name, first),
            );
            continue;
        }
        push_doc_line(
            &mut out,
            indent,
            &format!("<param name=\"{}\">", field.name),
        );
        push_text_line(&mut out, indent, first);
        for line in rest {
            push_text_line(&mut out, indent, line);
        }
        push_doc_line(&mut out, indent, "</param>");
    }
    out
}

fn push_doc_line(out: &mut String, indent: &str, payload: &str) {
    out.push_str(indent);
    out.push_str("/// ");
    out.push_str(payload);
    out.push('\n');
}

fn push_text_line(out: &mut String, indent: &str, line: &str) {
    out.push_str(indent);
    if line.is_empty() {
        out.push_str("///\n");
    } else {
        out.push_str("/// ");
        out.push_str(line);
        out.push('\n');
    }
}

/// Renders the file header: auto-generated comment, `using` directives,
/// and namespace declaration.
#[derive(Template)]
#[template(path = "render_csharp/preamble.txt", escape = "none")]
pub struct PreambleTemplate<'a> {
    pub module: &'a CSharpModulePlan,
}

/// Renders the public static wrapper class with methods that delegate
/// to the native P/Invoke declarations.
#[derive(Template)]
#[template(path = "render_csharp/functions.txt", escape = "none")]
pub struct FunctionsTemplate<'a> {
    pub module: &'a CSharpModulePlan,
}

/// Renders the `NativeMethods` static class containing `[DllImport]`
/// declarations for the C FFI functions.
#[derive(Template)]
#[template(path = "render_csharp/native.txt", escape = "none")]
pub struct NativeTemplate<'a> {
    pub module: &'a CSharpModulePlan,
}

/// Renders a single record as a standalone `.cs` file. Each record becomes
/// a `readonly record struct`, with a `[StructLayout(Sequential)]`
/// attribute for blittable records (passed directly across P/Invoke) and
/// wire encode/decode helpers for the wire-encoded path.
#[derive(Template)]
#[template(path = "render_csharp/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub record: &'a CSharpRecordPlan,
    pub namespace: &'a CSharpNamespace,
}

/// Renders a single C-style enum as a standalone `.cs` file: the native
/// `public enum` declaration plus the `*Wire` static helper class that
/// supplies `Decode` and the `WireEncodeTo` extension method. The enum
/// itself passes across P/Invoke as its declared integral backing type;
/// the wire helpers exist so records and data-enum variants embedding the
/// enum can stay on the same `this.Field.WireEncodeTo(wire)` call shape
/// as records.
#[derive(Template)]
#[template(path = "render_csharp/enum_c_style.txt", escape = "none")]
pub struct EnumCStyleTemplate<'a> {
    pub enumeration: &'a CSharpEnumPlan,
    pub namespace: &'a CSharpNamespace,
}

/// Renders a data enum as an `abstract record` with nested `sealed record`
/// variants. Closed hierarchy (private constructor), value equality per
/// variant, and pattern-matching wire codec using switch expressions for
/// the pure paths and a switch statement for the side-effecting encode.
#[derive(Template)]
#[template(path = "render_csharp/enum_data.txt", escape = "none")]
pub struct EnumDataTemplate<'a> {
    pub enumeration: &'a CSharpEnumPlan,
    pub namespace: &'a CSharpNamespace,
}

/// Renders a Rust class as a sealed C# class implementing `IDisposable`
/// around an opaque `IntPtr` handle. The wrapper takes ownership of the
/// handle, frees it through `NativeMethods.{Class}Free` on `Dispose`,
/// and falls back to the finalizer if the consumer forgets.
#[derive(Template)]
#[template(path = "render_csharp/class.txt", escape = "none")]
pub struct ClassTemplate<'a> {
    pub class: &'a CSharpClassPlan,
    pub namespace: &'a CSharpNamespace,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::csharp::ast::{
        CSharpArgumentList, CSharpBinaryOp, CSharpClassName, CSharpEnumUnderlyingType,
        CSharpExpression, CSharpIdentity, CSharpLiteral, CSharpLocalName, CSharpMethodName,
        CSharpParamName, CSharpPropertyName, CSharpStatement, CSharpType, CSharpTypeReference,
    };
    use crate::render::csharp::plan::{
        CFunctionName, CSharpClassPlan, CSharpConstructorKind, CSharpConstructorPlan,
        CSharpEnumKind, CSharpEnumPlan, CSharpEnumVariantPlan, CSharpFieldPlan, CSharpMethodPlan,
        CSharpParamKind, CSharpParamPlan, CSharpReceiver, CSharpRecordPlan, CSharpReturnKind,
    };

    fn demo_namespace() -> CSharpNamespace {
        CSharpNamespace::from_source("demo")
    }

    fn record_type(name: &str) -> CSharpType {
        CSharpType::Record(CSharpClassName::from_source(name).into())
    }

    fn c_style_enum_type(name: &str) -> CSharpType {
        CSharpType::CStyleEnum(CSharpClassName::from_source(name).into())
    }

    fn data_enum_type(name: &str) -> CSharpType {
        CSharpType::DataEnum(CSharpClassName::from_source(name).into())
    }

    /// `name` is the property name as it appears in generated C#
    /// (PascalCase). The test fixtures pass it in already-shaped
    /// because the generated code is what they're pinning.
    fn record_field(
        name: &str,
        csharp_type: CSharpType,
        decode: CSharpExpression,
        size: CSharpExpression,
        encode: CSharpStatement,
    ) -> CSharpFieldPlan {
        CSharpFieldPlan {
            summary_doc: None,
            name: CSharpPropertyName::from_source(name),
            csharp_type,
            wire_decode_expr: decode,
            wire_size_expr: size,
            wire_encode_stmts: vec![encode],
        }
    }

    fn local_ident(name: &str) -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new(name)))
    }

    /// `reader.{Method}()` — the canonical decode shape for primitives,
    /// `String`, and `Bytes`.
    fn read_call(method: &str) -> CSharpExpression {
        CSharpExpression::MethodCall {
            receiver: Box::new(local_ident("reader")),
            method: CSharpMethodName::from_source(method),
            type_args: vec![],
            args: CSharpArgumentList::default(),
        }
    }

    /// `{Class}.Decode(reader)` — the decode shape for record-typed and
    /// C-style-enum-wire-helper-typed fields.
    fn class_decode(class: &str) -> CSharpExpression {
        CSharpExpression::MethodCall {
            receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                CSharpClassName::new(class),
            ))),
            method: CSharpMethodName::from_source("decode"),
            type_args: vec![],
            args: vec![local_ident("reader")].into(),
        }
    }

    /// `n` — integer literal, the size shape for primitives and
    /// fixed-size composites.
    fn int_lit(n: i64) -> CSharpExpression {
        CSharpExpression::Literal(CSharpLiteral::Int(n))
    }

    /// `wire.{Method}(this.{Field})` as a statement — the encode shape
    /// for primitive-typed and string-typed record fields.
    fn wire_write_this(method: &str, field: &str) -> CSharpStatement {
        CSharpStatement::Expression(CSharpExpression::MethodCall {
            receiver: Box::new(local_ident("wire")),
            method: CSharpMethodName::from_source(method),
            type_args: vec![],
            args: vec![CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                name: CSharpPropertyName::from_source(field),
            }]
            .into(),
        })
    }

    /// `wire.{Method}({local}.{Field})` as a statement — variant of
    /// [`wire_write_this`] for variant payloads bound through a local
    /// (e.g. `_v.Radius`).
    fn wire_write_local_field(method: &str, local: &str, field: &str) -> CSharpStatement {
        CSharpStatement::Expression(CSharpExpression::MethodCall {
            receiver: Box::new(local_ident("wire")),
            method: CSharpMethodName::from_source(method),
            type_args: vec![],
            args: vec![CSharpExpression::MemberAccess {
                receiver: Box::new(local_ident(local)),
                name: CSharpPropertyName::from_source(field),
            }]
            .into(),
        })
    }

    /// `this.{Field}.WireEncodeTo(wire)` — the encode shape for nested
    /// record fields and C-style-enum-typed fields.
    fn this_wire_encode(field: &str) -> CSharpStatement {
        CSharpStatement::Expression(CSharpExpression::MethodCall {
            receiver: Box::new(CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                name: CSharpPropertyName::from_source(field),
            }),
            method: CSharpMethodName::new("WireEncodeTo"),
            type_args: vec![],
            args: vec![local_ident("wire")].into(),
        })
    }

    /// `wire.WriteString(this.{Field})` is the same shape as
    /// [`wire_write_this`] but the encoded-size contribution for a
    /// string field is the bespoke `(4 + Encoding.UTF8.GetByteCount(this.{Field}))`.
    fn string_size_this(field: &str) -> CSharpExpression {
        let utf8 = CSharpExpression::MemberAccess {
            receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(
                CSharpClassName::new("Encoding"),
            ))),
            name: CSharpPropertyName::from_source("UTF8"),
        };
        let byte_count = CSharpExpression::MethodCall {
            receiver: Box::new(utf8),
            method: CSharpMethodName::new("GetByteCount"),
            type_args: vec![],
            args: vec![CSharpExpression::MemberAccess {
                receiver: Box::new(CSharpExpression::Identity(CSharpIdentity::This)),
                name: CSharpPropertyName::from_source(field),
            }]
            .into(),
        };
        CSharpExpression::Paren(Box::new(CSharpExpression::Binary {
            op: CSharpBinaryOp::Add,
            left: Box::new(int_lit(4)),
            right: Box::new(byte_count),
        }))
    }

    /// Point: the canonical blittable record. Two f64 fields, `#[repr(C)]`
    /// in Rust. Carries `[StructLayout(Sequential)]` and still emits wire
    /// helpers so it can be embedded inside a non-blittable record's
    /// wire encode/decode path without a second code shape.
    #[test]
    fn snapshot_blittable_record_point() {
        let record = CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("point"),
            is_blittable: true,
            fields: vec![
                record_field(
                    "X",
                    CSharpType::Double,
                    read_call("read_f64"),
                    int_lit(8),
                    wire_write_this("write_f64", "X"),
                ),
                record_field(
                    "Y",
                    CSharpType::Double,
                    read_call("read_f64"),
                    int_lit(8),
                    wire_write_this("write_f64", "Y"),
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Person: the canonical non-blittable record: a string field (which
    /// forces the wire path) plus a primitive. No StructLayout attribute.
    /// Imports `System.Text` because the size expression uses
    /// `Encoding.UTF8.GetByteCount`.
    #[test]
    fn snapshot_non_blittable_record_person_with_string() {
        let record = CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("person"),
            is_blittable: false,
            fields: vec![
                record_field(
                    "Name",
                    CSharpType::String,
                    read_call("read_string"),
                    string_size_this("Name"),
                    wire_write_this("write_string", "Name"),
                ),
                record_field(
                    "Age",
                    CSharpType::UInt,
                    read_call("read_u32"),
                    int_lit(4),
                    wire_write_this("write_u32", "Age"),
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Line: a record whose fields are themselves records. The decode
    /// expression for a record-typed field is `Point.Decode(reader)` and
    /// the encode is `this.Start.WireEncodeTo(wire)`, the recursive
    /// glue that lets records compose.
    #[test]
    fn snapshot_nested_record_line() {
        let record = CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("line"),
            is_blittable: false,
            fields: vec![
                record_field(
                    "Start",
                    record_type("point"),
                    class_decode("Point"),
                    int_lit(16),
                    this_wire_encode("Start"),
                ),
                record_field(
                    "End",
                    record_type("point"),
                    class_decode("Point"),
                    int_lit(16),
                    this_wire_encode("End"),
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// A fieldless record: the template must still produce valid C#.
    /// `WireEncodedSize` returns 0 and `WireEncodeTo` is an empty method.
    #[test]
    fn snapshot_empty_record() {
        let record = CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("unit"),
            is_blittable: true,
            fields: vec![],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Flag: the canonical "blittable record with a C-style enum field."
    /// Status is an `enum : int` here, so embedding it alongside a `uint`
    /// keeps the record on the zero-copy P/Invoke path with
    /// `[StructLayout(Sequential)]`. The wire helpers are still emitted
    /// so non-blittable records that embed `Flag` can reach its
    /// wire encoder without a second rendering shape.
    #[test]
    fn snapshot_blittable_record_with_cstyle_enum_field() {
        let record = CSharpRecordPlan {
            summary_doc: None,
            class_name: CSharpClassName::from_source("flag"),
            is_blittable: true,
            fields: vec![
                record_field(
                    "Status",
                    c_style_enum_type("status"),
                    class_decode("StatusWire"),
                    int_lit(4),
                    this_wire_encode("Status"),
                ),
                record_field(
                    "Count",
                    CSharpType::UInt,
                    read_call("read_u32"),
                    int_lit(4),
                    wire_write_this("write_u32", "Count"),
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// `owner_class_name` and `name` are already in their rendered C#
    /// form — the fixtures pin what the generated code looks like, so
    /// we don't run a source transform on them.
    ///
    /// Mirrors the lowerer's contract for `InstanceNative` receivers:
    /// the receiver's wire-encode block lives at `wire_writers[0]`, so
    /// the snapshot fixtures synthesize the same plan the lowerer would.
    fn method(
        owner_class_name: &str,
        name: &str,
        ffi_name: &str,
        receiver: CSharpReceiver,
        params: Vec<CSharpParamPlan>,
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpMethodPlan {
        let owner = CSharpClassName::from_source(owner_class_name);
        let method_name = CSharpMethodName::from_source(name);
        let wire_writers = if matches!(receiver, CSharpReceiver::InstanceNative) {
            vec![crate::render::csharp::lower::self_wire_writer()]
        } else {
            vec![]
        };
        CSharpMethodPlan {
            summary_doc: None,
            native_method_name: CSharpMethodName::native_for_owner(&owner, &method_name),
            name: method_name,
            ffi_name: CFunctionName::new(ffi_name.to_string()),
            receiver,
            params,
            return_type,
            return_kind,
            wire_writers,
        }
    }

    fn param(name: &str, csharp_type: CSharpType) -> CSharpParamPlan {
        CSharpParamPlan {
            name: CSharpParamName::from_source(name),
            csharp_type,
            kind: CSharpParamKind::Direct,
        }
    }

    /// Build a CSharpEnumPlan for the snapshot fixtures. Accepts the
    /// already-rendered class name and populates the derived
    /// `wire_class_name` and `methods_class_name` to match what the
    /// lowerer would produce.
    fn build_enum(
        class_source: &str,
        kind: CSharpEnumKind,
        underlying_type: Option<CSharpEnumUnderlyingType>,
        variants: Vec<CSharpEnumVariantPlan>,
        methods: Vec<CSharpMethodPlan>,
    ) -> CSharpEnumPlan {
        let class_name = CSharpClassName::from_source(class_source);
        let wire_class_name = CSharpClassName::wire_helper(&class_name);
        let methods_class_name = if methods.is_empty() {
            None
        } else {
            Some(CSharpClassName::methods_companion(&class_name))
        };
        CSharpEnumPlan {
            summary_doc: None,
            class_name,
            wire_class_name,
            methods_class_name,
            kind,
            underlying_type,
            variants,
            methods,
        }
    }

    fn variant(
        name: &str,
        tag: i32,
        wire_tag: i32,
        fields: Vec<CSharpFieldPlan>,
    ) -> CSharpEnumVariantPlan {
        CSharpEnumVariantPlan {
            summary_doc: None,
            name: CSharpClassName::from_source(name),
            tag,
            wire_tag,
            fields,
        }
    }

    /// Direction: C-style enum with a mix of static factories and
    /// instance methods. Renders alongside the `DirectionWire` helper
    /// plus a `DirectionMethods` companion static class; instance
    /// methods become C# extension methods (`this Direction self`) so
    /// `d.Opposite()` works without members on the enum itself.
    #[test]
    fn snapshot_c_style_enum_with_methods_direction() {
        let variants = vec![
            variant("North", 0, 0, vec![]),
            variant("South", 1, 1, vec![]),
            variant("East", 2, 2, vec![]),
            variant("West", 3, 3, vec![]),
        ];
        let methods = vec![
            method(
                "Direction",
                "FromDegrees",
                "boltffi_direction_from_degrees",
                CSharpReceiver::Static,
                vec![param("degrees", CSharpType::Double)],
                c_style_enum_type("direction"),
                CSharpReturnKind::Direct,
            ),
            method(
                "Direction",
                "Count",
                "boltffi_direction_count",
                CSharpReceiver::Static,
                vec![],
                CSharpType::UInt,
                CSharpReturnKind::Direct,
            ),
            method(
                "Direction",
                "Opposite",
                "boltffi_direction_opposite",
                CSharpReceiver::InstanceExtension,
                vec![],
                c_style_enum_type("direction"),
                CSharpReturnKind::Direct,
            ),
            method(
                "Direction",
                "IsHorizontal",
                "boltffi_direction_is_horizontal",
                CSharpReceiver::InstanceExtension,
                vec![],
                CSharpType::Bool,
                CSharpReturnKind::Direct,
            ),
            method(
                "Direction",
                "Label",
                "boltffi_direction_label",
                CSharpReceiver::InstanceExtension,
                vec![],
                CSharpType::String,
                CSharpReturnKind::WireDecodeString,
            ),
        ];
        let enumeration = build_enum(
            "direction",
            CSharpEnumKind::CStyle,
            Some(CSharpEnumUnderlyingType::Int),
            variants,
            methods,
        );
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Status: the canonical C-style enum. Three unit variants with
    /// ordinal-index tags. Renders as a native `public enum : int` plus
    /// the `StatusWire` static helper with the `WireEncodeTo` extension.
    #[test]
    fn snapshot_c_style_enum_status() {
        let variants = vec![
            variant("Active", 0, 0, vec![]),
            variant("Inactive", 1, 1, vec![]),
            variant("Pending", 2, 2, vec![]),
        ];
        let enumeration = build_enum(
            "status",
            CSharpEnumKind::CStyle,
            Some(CSharpEnumUnderlyingType::Int),
            variants,
            vec![],
        );
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// LogLevel: a non-default C-style enum backing type. The C# surface
    /// must preserve the `byte` backing type so direct P/Invoke matches
    /// Rust's `#[repr(u8)]`, and the wire helper must use the 1-byte read /
    /// write ops rather than hard-coding `I32`.
    #[test]
    fn snapshot_c_style_enum_log_level_u8() {
        let variants = vec![
            variant("Trace", 0, 0, vec![]),
            variant("Debug", 1, 1, vec![]),
            variant("Info", 2, 2, vec![]),
            variant("Warn", 3, 3, vec![]),
            variant("Error", 4, 4, vec![]),
        ];
        let enumeration = build_enum(
            "log_level",
            CSharpEnumKind::CStyle,
            Some(CSharpEnumUnderlyingType::Byte),
            variants,
            vec![],
        );
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Shape: the canonical data enum exercising every payload shape:
    /// a single-field variant (Circle), a multi-field variant (Rectangle),
    /// and a unit variant (Point). Renders as `abstract record Shape`
    /// with nested `sealed record` variants, switch-expression Decode,
    /// and pattern-match WireEncodeTo. Field wire expressions reference
    /// the switch-bound local `_v`, not `this`.
    #[test]
    fn snapshot_data_enum_shape() {
        let variants = vec![
            variant(
                "Circle",
                0,
                0,
                vec![record_field(
                    "Radius",
                    CSharpType::Double,
                    read_call("read_f64"),
                    int_lit(8),
                    wire_write_local_field("write_f64", "_v", "Radius"),
                )],
            ),
            variant(
                "Rectangle",
                1,
                1,
                vec![
                    record_field(
                        "Width",
                        CSharpType::Double,
                        read_call("read_f64"),
                        int_lit(8),
                        wire_write_local_field("write_f64", "_v", "Width"),
                    ),
                    record_field(
                        "Height",
                        CSharpType::Double,
                        read_call("read_f64"),
                        int_lit(8),
                        wire_write_local_field("write_f64", "_v", "Height"),
                    ),
                ],
            ),
            variant("Point", 2, 2, vec![]),
        ];
        let enumeration = build_enum("shape", CSharpEnumKind::Data, None, variants, vec![]);
        let template = EnumDataTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Shape with methods: a data enum carrying both static factories
    /// (UnitCircle, VariantCount) and instance methods (Area, Describe).
    /// Methods render inside the abstract record body. Instance ones
    /// wire-encode `this` into `_selfBytes` before the native call.
    #[test]
    fn snapshot_data_enum_with_methods_shape() {
        let variants = vec![variant(
            "Circle",
            0,
            0,
            vec![record_field(
                "Radius",
                CSharpType::Double,
                read_call("read_f64"),
                int_lit(8),
                wire_write_local_field("write_f64", "_v", "Radius"),
            )],
        )];
        let methods = vec![
            method(
                "Shape",
                "UnitCircle",
                "boltffi_shape_unit_circle",
                CSharpReceiver::Static,
                vec![],
                data_enum_type("shape"),
                CSharpReturnKind::WireDecodeObject {
                    class_name: CSharpClassName::from_source("shape"),
                },
            ),
            method(
                "Shape",
                "VariantCount",
                "boltffi_shape_variant_count",
                CSharpReceiver::Static,
                vec![],
                CSharpType::UInt,
                CSharpReturnKind::Direct,
            ),
            method(
                "Shape",
                "Area",
                "boltffi_shape_area",
                CSharpReceiver::InstanceNative,
                vec![],
                CSharpType::Double,
                CSharpReturnKind::Direct,
            ),
            method(
                "Shape",
                "Describe",
                "boltffi_shape_describe",
                CSharpReceiver::InstanceNative,
                vec![],
                CSharpType::String,
                CSharpReturnKind::WireDecodeString,
            ),
        ];
        let enumeration = build_enum("shape", CSharpEnumKind::Data, None, variants, methods);
        let template = EnumDataTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Inventory: a Rust class with no constructors or methods exposed
    /// in the plan yet. Pins the bare IDisposable wrapper: the handle
    /// is held in a private `IntPtr` and freed exactly once through
    /// `NativeMethods.InventoryFree`, with the finalizer as a safety
    /// net for callers that forget to dispose.
    #[test]
    fn snapshot_class_inventory_idisposable_wrapper() {
        let class_name = CSharpClassName::from_source("inventory");
        let class = CSharpClassPlan {
            summary_doc: None,
            native_free_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &CSharpMethodName::new("Free"),
            ),
            class_name,
            ffi_free: CFunctionName::new("boltffi_inventory_free".to_string()),
            constructors: vec![],
            methods: vec![],
        };
        let template = ClassTemplate {
            class: &class,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Inventory with both a `Default` constructor (`new`) and a
    /// `NamedInit` constructor (`with_capacity(u32)`). Pins the two
    /// rendering shapes side by side: the primary lifts to a real C#
    /// instance constructor delegating through a private static
    /// helper, and the named-init lifts to a `public static` factory
    /// that wraps the returned `IntPtr`.
    #[test]
    fn snapshot_class_inventory_with_constructors() {
        let class_name = CSharpClassName::from_source("inventory");
        let primary = CSharpConstructorPlan {
            summary_doc: None,
            kind: CSharpConstructorKind::Primary {
                helper_method_name: CSharpMethodName::new("InventoryNewHandle"),
            },
            native_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &CSharpMethodName::new("New"),
            ),
            ffi_name: CFunctionName::new("boltffi_inventory_new".to_string()),
            params: vec![],
            wire_writers: vec![],
        };
        let with_capacity_name = CSharpMethodName::from_source("with_capacity");
        let factory = CSharpConstructorPlan {
            summary_doc: None,
            kind: CSharpConstructorKind::StaticFactory {
                name: with_capacity_name.clone(),
            },
            native_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &with_capacity_name,
            ),
            ffi_name: CFunctionName::new("boltffi_inventory_with_capacity".to_string()),
            params: vec![CSharpParamPlan {
                name: CSharpParamName::from_source("capacity"),
                csharp_type: CSharpType::UInt,
                kind: CSharpParamKind::Direct,
            }],
            wire_writers: vec![],
        };
        let class = CSharpClassPlan {
            summary_doc: None,
            native_free_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &CSharpMethodName::new("Free"),
            ),
            class_name,
            ffi_free: CFunctionName::new("boltffi_inventory_free".to_string()),
            constructors: vec![primary, factory],
            methods: vec![],
        };
        let template = ClassTemplate {
            class: &class,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Counter with two instance methods (a getter returning a
    /// primitive and a void mutator) and a static method. Pins the
    /// three rendering shapes the class template emits for methods:
    ///
    /// - Static: `public static {ReturnType} {Name}(...)` body.
    /// - ClassInstance void: `NativeMethods.{Name}(_handle, ...)`.
    /// - ClassInstance primitive return: `return NativeMethods.{Name}(_handle, ...)`.
    #[test]
    fn snapshot_class_counter_with_methods() {
        let class_name = CSharpClassName::from_source("counter");
        let get_name = CSharpMethodName::from_source("get");
        let get = CSharpMethodPlan {
            summary_doc: None,
            name: get_name.clone(),
            native_method_name: CSharpMethodName::native_for_owner(&class_name, &get_name),
            ffi_name: CFunctionName::new("boltffi_counter_get".to_string()),
            receiver: CSharpReceiver::ClassInstance,
            params: vec![],
            return_type: CSharpType::Int,
            return_kind: CSharpReturnKind::Direct,
            wire_writers: vec![],
        };
        let increment_name = CSharpMethodName::from_source("increment");
        let increment = CSharpMethodPlan {
            summary_doc: None,
            name: increment_name.clone(),
            native_method_name: CSharpMethodName::native_for_owner(&class_name, &increment_name),
            ffi_name: CFunctionName::new("boltffi_counter_increment".to_string()),
            receiver: CSharpReceiver::ClassInstance,
            params: vec![],
            return_type: CSharpType::Void,
            return_kind: CSharpReturnKind::Void,
            wire_writers: vec![],
        };
        let zero_name = CSharpMethodName::from_source("zero");
        let zero = CSharpMethodPlan {
            summary_doc: None,
            name: zero_name.clone(),
            native_method_name: CSharpMethodName::native_for_owner(&class_name, &zero_name),
            ffi_name: CFunctionName::new("boltffi_counter_zero".to_string()),
            receiver: CSharpReceiver::Static,
            params: vec![],
            return_type: CSharpType::Int,
            return_kind: CSharpReturnKind::Direct,
            wire_writers: vec![],
        };
        let class = CSharpClassPlan {
            summary_doc: None,
            native_free_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &CSharpMethodName::new("Free"),
            ),
            class_name,
            ffi_free: CFunctionName::new("boltffi_counter_free".to_string()),
            constructors: vec![],
            methods: vec![get, increment, zero],
        };
        let template = ClassTemplate {
            class: &class,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    fn doc(text: &str) -> Option<CSharpComment> {
        CSharpComment::from_str_option(Some(text))
    }

    /// Person with docs on the record itself and on each field. Pins the
    /// `<summary>` block above the declaration and one `<param name="...">`
    /// per documented field, both indented to four spaces (record) and
    /// kept above the positional record's `(...)` parameter list. The
    /// multi-line summary preserves blank lines as bare `///` separators,
    /// and `<` / `&` in the body render as `&lt;` / `&amp;` since the
    /// helper escapes XML special characters at construction.
    #[test]
    fn snapshot_record_person_with_docs() {
        let record = CSharpRecordPlan {
            summary_doc: doc("A person record.\n\nWraps Vec<String> & friends."),
            class_name: CSharpClassName::from_source("person"),
            is_blittable: false,
            fields: vec![
                CSharpFieldPlan {
                    summary_doc: doc("The display name."),
                    ..record_field(
                        "Name",
                        CSharpType::String,
                        read_call("read_string"),
                        string_size_this("Name"),
                        wire_write_this("write_string", "Name"),
                    )
                },
                CSharpFieldPlan {
                    summary_doc: doc("Age in years."),
                    ..record_field(
                        "Age",
                        CSharpType::UInt,
                        read_call("read_u32"),
                        int_lit(4),
                        wire_write_this("write_u32", "Age"),
                    )
                },
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Status with a class-level doc and per-variant docs. Pins the
    /// `<summary>` block above `public enum Status` (indent 4) and above
    /// each variant line (indent 8), with the helper-companion still
    /// emitted alongside.
    #[test]
    fn snapshot_c_style_enum_status_with_docs() {
        let variants = vec![
            CSharpEnumVariantPlan {
                summary_doc: doc("In active use."),
                ..variant("Active", 0, 0, vec![])
            },
            CSharpEnumVariantPlan {
                summary_doc: doc("Soft-deleted."),
                ..variant("Inactive", 1, 1, vec![])
            },
            CSharpEnumVariantPlan {
                summary_doc: doc("Awaiting review."),
                ..variant("Pending", 2, 2, vec![])
            },
        ];
        let mut enumeration = build_enum(
            "status",
            CSharpEnumKind::CStyle,
            Some(CSharpEnumUnderlyingType::Int),
            variants,
            vec![],
        );
        enumeration.summary_doc = doc("Lifecycle status of an item.");
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Shape with docs on the data enum, on each variant, and on the
    /// payload fields. Pins the three doc paths the data-enum template
    /// touches: namespace-indented `<summary>` above
    /// `public abstract record Shape`, variant-indented `<summary>`
    /// above each `public sealed record …`, and `<param name="…">`
    /// blocks for the positional payload fields.
    #[test]
    fn snapshot_data_enum_shape_with_docs() {
        let variants = vec![
            CSharpEnumVariantPlan {
                summary_doc: doc("A round shape."),
                ..variant(
                    "Circle",
                    0,
                    0,
                    vec![CSharpFieldPlan {
                        summary_doc: doc("Distance from the center."),
                        ..record_field(
                            "Radius",
                            CSharpType::Double,
                            read_call("read_f64"),
                            int_lit(8),
                            wire_write_local_field("write_f64", "_v", "Radius"),
                        )
                    }],
                )
            },
            CSharpEnumVariantPlan {
                summary_doc: doc("A degenerate point."),
                ..variant("Point", 1, 1, vec![])
            },
        ];
        let mut enumeration = build_enum("shape", CSharpEnumKind::Data, None, variants, vec![]);
        enumeration.summary_doc = doc("A 2D shape.");
        let template = EnumDataTemplate {
            enumeration: &enumeration,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Counter with docs on the class, on a primary constructor, and on
    /// instance + static methods. Pins the doc block above the class
    /// declaration, above the `public Counter(...)` ctor, and above each
    /// of the three method shapes (static, ClassInstance void,
    /// ClassInstance returning a primitive).
    #[test]
    fn snapshot_class_counter_with_docs() {
        let class_name = CSharpClassName::from_source("counter");
        let primary = CSharpConstructorPlan {
            summary_doc: doc("Creates a counter starting at zero."),
            kind: CSharpConstructorKind::Primary {
                helper_method_name: CSharpMethodName::new("CounterNewHandle"),
            },
            native_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &CSharpMethodName::new("New"),
            ),
            ffi_name: CFunctionName::new("boltffi_counter_new".to_string()),
            params: vec![],
            wire_writers: vec![],
        };
        let get_name = CSharpMethodName::from_source("get");
        let get = CSharpMethodPlan {
            summary_doc: doc("Returns the current value."),
            name: get_name.clone(),
            native_method_name: CSharpMethodName::native_for_owner(&class_name, &get_name),
            ffi_name: CFunctionName::new("boltffi_counter_get".to_string()),
            receiver: CSharpReceiver::ClassInstance,
            params: vec![],
            return_type: CSharpType::Int,
            return_kind: CSharpReturnKind::Direct,
            wire_writers: vec![],
        };
        let increment_name = CSharpMethodName::from_source("increment");
        let increment = CSharpMethodPlan {
            summary_doc: doc("Adds one to the current value."),
            name: increment_name.clone(),
            native_method_name: CSharpMethodName::native_for_owner(&class_name, &increment_name),
            ffi_name: CFunctionName::new("boltffi_counter_increment".to_string()),
            receiver: CSharpReceiver::ClassInstance,
            params: vec![],
            return_type: CSharpType::Void,
            return_kind: CSharpReturnKind::Void,
            wire_writers: vec![],
        };
        let zero_name = CSharpMethodName::from_source("zero");
        let zero = CSharpMethodPlan {
            summary_doc: doc("Static factory returning zero."),
            name: zero_name.clone(),
            native_method_name: CSharpMethodName::native_for_owner(&class_name, &zero_name),
            ffi_name: CFunctionName::new("boltffi_counter_zero".to_string()),
            receiver: CSharpReceiver::Static,
            params: vec![],
            return_type: CSharpType::Int,
            return_kind: CSharpReturnKind::Direct,
            wire_writers: vec![],
        };
        let class = CSharpClassPlan {
            summary_doc: doc("Mutable counter held over FFI."),
            native_free_method_name: CSharpMethodName::native_for_owner(
                &class_name,
                &CSharpMethodName::new("Free"),
            ),
            class_name,
            ffi_free: CFunctionName::new("boltffi_counter_free".to_string()),
            constructors: vec![primary],
            methods: vec![get, increment, zero],
        };
        let template = ClassTemplate {
            class: &class,
            namespace: &demo_namespace(),
        };
        insta::assert_snapshot!(template.render().unwrap());
    }
}
