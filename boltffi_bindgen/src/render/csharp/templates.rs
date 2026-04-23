//! Askama template bindings: one struct per `.txt` template, each
//! borrowing the plan node it renders. Templates only interpolate.
//! All branching and decision logic lives upstream in `lower`.
//!
//! Snapshot tests pin the rendered shape against curated plan
//! fixtures, so a template change surfaces as a single `.snap` diff
//! rather than rippling across emit-level tests.

use askama::Template;

use super::plan::{CSharpEnum, CSharpModule, CSharpRecord};

/// Renders the file header: auto-generated comment, `using` directives,
/// and namespace declaration.
#[derive(Template)]
#[template(path = "render_csharp/preamble.txt", escape = "none")]
pub struct PreambleTemplate<'a> {
    pub module: &'a CSharpModule,
}

/// Renders the public static wrapper class with methods that delegate
/// to the native P/Invoke declarations.
#[derive(Template)]
#[template(path = "render_csharp/functions.txt", escape = "none")]
pub struct FunctionsTemplate<'a> {
    pub module: &'a CSharpModule,
}

/// Renders the `NativeMethods` static class containing `[DllImport]`
/// declarations for the C FFI functions.
#[derive(Template)]
#[template(path = "render_csharp/native.txt", escape = "none")]
pub struct NativeTemplate<'a> {
    pub module: &'a CSharpModule,
}

/// Renders a single record as a standalone `.cs` file. Each record becomes
/// a `readonly record struct`, with a `[StructLayout(Sequential)]`
/// attribute for blittable records (passed directly across P/Invoke) and
/// wire encode/decode helpers for the wire-encoded path.
#[derive(Template)]
#[template(path = "render_csharp/record.txt", escape = "none")]
pub struct RecordTemplate<'a> {
    pub record: &'a CSharpRecord,
    pub namespace: &'a str,
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
    pub enumeration: &'a CSharpEnum,
    pub namespace: &'a str,
}

/// Renders a data enum as an `abstract record` with nested `sealed record`
/// variants. Closed hierarchy (private constructor), value equality per
/// variant, and pattern-matching wire codec using switch expressions for
/// the pure paths and a switch statement for the side-effecting encode.
#[derive(Template)]
#[template(path = "render_csharp/enum_data.txt", escape = "none")]
pub struct EnumDataTemplate<'a> {
    pub enumeration: &'a CSharpEnum,
    pub namespace: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::csharp::plan::{
        CSharpEnum, CSharpEnumKind, CSharpEnumVariant, CSharpMethod, CSharpParam, CSharpParamKind,
        CSharpReceiver, CSharpRecord, CSharpRecordField, CSharpReturnKind, CSharpType,
    };

    fn record_field(
        name: &str,
        csharp_type: CSharpType,
        decode: &str,
        size: &str,
        encode: &str,
    ) -> CSharpRecordField {
        CSharpRecordField {
            name: name.to_string(),
            csharp_type,
            wire_decode_expr: decode.to_string(),
            wire_size_expr: size.to_string(),
            wire_encode_expr: encode.to_string(),
        }
    }

    /// Point: the canonical blittable record. Two f64 fields, `#[repr(C)]`
    /// in Rust. Carries `[StructLayout(Sequential)]` and still emits wire
    /// helpers so it can be embedded inside a non-blittable record's
    /// wire encode/decode path without a second code shape.
    #[test]
    fn snapshot_blittable_record_point() {
        let record = CSharpRecord {
            class_name: "Point".to_string(),
            is_blittable: true,
            fields: vec![
                record_field(
                    "X",
                    CSharpType::Double,
                    "reader.ReadF64()",
                    "8",
                    "wire.WriteF64(this.X)",
                ),
                record_field(
                    "Y",
                    CSharpType::Double,
                    "reader.ReadF64()",
                    "8",
                    "wire.WriteF64(this.Y)",
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Person: the canonical non-blittable record: a string field (which
    /// forces the wire path) plus a primitive. No StructLayout attribute.
    /// Imports `System.Text` because the size expression uses
    /// `Encoding.UTF8.GetByteCount`.
    #[test]
    fn snapshot_non_blittable_record_person_with_string() {
        let record = CSharpRecord {
            class_name: "Person".to_string(),
            is_blittable: false,
            fields: vec![
                record_field(
                    "Name",
                    CSharpType::String,
                    "reader.ReadString()",
                    "(4 + Encoding.UTF8.GetByteCount(this.Name))",
                    "wire.WriteString(this.Name)",
                ),
                record_field(
                    "Age",
                    CSharpType::UInt,
                    "reader.ReadU32()",
                    "4",
                    "wire.WriteU32(this.Age)",
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Line: a record whose fields are themselves records. The decode
    /// expression for a record-typed field is `Point.Decode(reader)` and
    /// the encode is `this.Start.WireEncodeTo(wire)`, the recursive
    /// glue that lets records compose.
    #[test]
    fn snapshot_nested_record_line() {
        let record = CSharpRecord {
            class_name: "Line".to_string(),
            is_blittable: false,
            fields: vec![
                record_field(
                    "Start",
                    CSharpType::Record("Point".to_string()),
                    "Point.Decode(reader)",
                    "16",
                    "this.Start.WireEncodeTo(wire)",
                ),
                record_field(
                    "End",
                    CSharpType::Record("Point".to_string()),
                    "Point.Decode(reader)",
                    "16",
                    "this.End.WireEncodeTo(wire)",
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// A fieldless record: the template must still produce valid C#.
    /// `WireEncodedSize` returns 0 and `WireEncodeTo` is an empty method.
    #[test]
    fn snapshot_empty_record() {
        let record = CSharpRecord {
            class_name: "Unit".to_string(),
            is_blittable: true,
            fields: vec![],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: "Demo",
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
        let record = CSharpRecord {
            class_name: "Flag".to_string(),
            is_blittable: true,
            fields: vec![
                record_field(
                    "Status",
                    CSharpType::CStyleEnum("Status".to_string()),
                    "StatusWire.Decode(reader)",
                    "4",
                    "this.Status.WireEncodeTo(wire)",
                ),
                record_field(
                    "Count",
                    CSharpType::UInt,
                    "reader.ReadU32()",
                    "4",
                    "wire.WriteU32(this.Count)",
                ),
            ],
        };
        let template = RecordTemplate {
            record: &record,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    fn method(
        owner_class_name: &str,
        name: &str,
        ffi_name: &str,
        receiver: CSharpReceiver,
        params: Vec<CSharpParam>,
        return_type: CSharpType,
        return_kind: CSharpReturnKind,
    ) -> CSharpMethod {
        CSharpMethod {
            name: name.to_string(),
            native_method_name: format!("{owner_class_name}{name}"),
            ffi_name: ffi_name.to_string(),
            receiver,
            params,
            return_type,
            return_kind,
            wire_writers: vec![],
        }
    }

    fn param(name: &str, csharp_type: CSharpType) -> CSharpParam {
        CSharpParam {
            name: name.to_string(),
            csharp_type,
            kind: CSharpParamKind::Direct,
        }
    }

    /// Direction: C-style enum with a mix of static factories and
    /// instance methods. Renders alongside the `DirectionWire` helper
    /// plus a `DirectionMethods` companion static class; instance
    /// methods become C# extension methods (`this Direction self`) so
    /// `d.Opposite()` works without members on the enum itself.
    #[test]
    fn snapshot_c_style_enum_with_methods_direction() {
        let enumeration = CSharpEnum {
            class_name: "Direction".to_string(),
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(crate::ir::types::PrimitiveType::I32),
            variants: vec![
                CSharpEnumVariant {
                    name: "North".to_string(),
                    tag: 0,
                    wire_tag: 0,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "South".to_string(),
                    tag: 1,
                    wire_tag: 1,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "East".to_string(),
                    tag: 2,
                    wire_tag: 2,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "West".to_string(),
                    tag: 3,
                    wire_tag: 3,
                    fields: vec![],
                },
            ],
            methods: vec![
                method(
                    "Direction",
                    "FromDegrees",
                    "boltffi_direction_from_degrees",
                    CSharpReceiver::Static,
                    vec![param("degrees", CSharpType::Double)],
                    CSharpType::CStyleEnum("Direction".to_string()),
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
                    CSharpType::CStyleEnum("Direction".to_string()),
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
            ],
        };
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Status: the canonical C-style enum. Three unit variants with
    /// ordinal-index tags. Renders as a native `public enum : int` plus
    /// the `StatusWire` static helper with the `WireEncodeTo` extension.
    #[test]
    fn snapshot_c_style_enum_status() {
        let enumeration = CSharpEnum {
            class_name: "Status".to_string(),
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(crate::ir::types::PrimitiveType::I32),
            variants: vec![
                CSharpEnumVariant {
                    name: "Active".to_string(),
                    tag: 0,
                    wire_tag: 0,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "Inactive".to_string(),
                    tag: 1,
                    wire_tag: 1,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "Pending".to_string(),
                    tag: 2,
                    wire_tag: 2,
                    fields: vec![],
                },
            ],
            methods: vec![],
        };
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// LogLevel: a non-default C-style enum backing type. The C# surface
    /// must preserve the `byte` backing type so direct P/Invoke matches
    /// Rust's `#[repr(u8)]`, and the wire helper must use the 1-byte read /
    /// write ops rather than hard-coding `I32`.
    #[test]
    fn snapshot_c_style_enum_log_level_u8() {
        let enumeration = CSharpEnum {
            class_name: "LogLevel".to_string(),
            kind: CSharpEnumKind::CStyle,
            c_style_tag_type: Some(crate::ir::types::PrimitiveType::U8),
            variants: vec![
                CSharpEnumVariant {
                    name: "Trace".to_string(),
                    tag: 0,
                    wire_tag: 0,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "Debug".to_string(),
                    tag: 1,
                    wire_tag: 1,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "Info".to_string(),
                    tag: 2,
                    wire_tag: 2,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "Warn".to_string(),
                    tag: 3,
                    wire_tag: 3,
                    fields: vec![],
                },
                CSharpEnumVariant {
                    name: "Error".to_string(),
                    tag: 4,
                    wire_tag: 4,
                    fields: vec![],
                },
            ],
            methods: vec![],
        };
        let template = EnumCStyleTemplate {
            enumeration: &enumeration,
            namespace: "Demo",
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
        let enumeration = CSharpEnum {
            class_name: "Shape".to_string(),
            kind: CSharpEnumKind::Data,
            c_style_tag_type: None,
            variants: vec![
                CSharpEnumVariant {
                    name: "Circle".to_string(),
                    tag: 0,
                    wire_tag: 0,
                    fields: vec![record_field(
                        "Radius",
                        CSharpType::Double,
                        "reader.ReadF64()",
                        "8",
                        "wire.WriteF64(_v.Radius)",
                    )],
                },
                CSharpEnumVariant {
                    name: "Rectangle".to_string(),
                    tag: 1,
                    wire_tag: 1,
                    fields: vec![
                        record_field(
                            "Width",
                            CSharpType::Double,
                            "reader.ReadF64()",
                            "8",
                            "wire.WriteF64(_v.Width)",
                        ),
                        record_field(
                            "Height",
                            CSharpType::Double,
                            "reader.ReadF64()",
                            "8",
                            "wire.WriteF64(_v.Height)",
                        ),
                    ],
                },
                CSharpEnumVariant {
                    name: "Point".to_string(),
                    tag: 2,
                    wire_tag: 2,
                    fields: vec![],
                },
            ],
            methods: vec![],
        };
        let template = EnumDataTemplate {
            enumeration: &enumeration,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }

    /// Shape with methods: a data enum carrying both static factories
    /// (UnitCircle, VariantCount) and instance methods (Area, Describe).
    /// Methods render inside the abstract record body. Instance ones
    /// wire-encode `this` into `_selfBytes` before the native call.
    #[test]
    fn snapshot_data_enum_with_methods_shape() {
        let enumeration = CSharpEnum {
            class_name: "Shape".to_string(),
            kind: CSharpEnumKind::Data,
            c_style_tag_type: None,
            variants: vec![CSharpEnumVariant {
                name: "Circle".to_string(),
                tag: 0,
                wire_tag: 0,
                fields: vec![record_field(
                    "Radius",
                    CSharpType::Double,
                    "reader.ReadF64()",
                    "8",
                    "wire.WriteF64(_v.Radius)",
                )],
            }],
            methods: vec![
                method(
                    "Shape",
                    "UnitCircle",
                    "boltffi_shape_unit_circle",
                    CSharpReceiver::Static,
                    vec![],
                    CSharpType::DataEnum("Shape".to_string()),
                    CSharpReturnKind::WireDecodeObject {
                        class_name: "Shape".to_string(),
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
            ],
        };
        let template = EnumDataTemplate {
            enumeration: &enumeration,
            namespace: "Demo",
        };
        insta::assert_snapshot!(template.render().unwrap());
    }
}
