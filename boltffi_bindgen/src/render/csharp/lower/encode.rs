//! Translate an IR [`WriteSeq`] into one or more C# encode-phase
//! statements. The IR names the phase `Write`/`WriteSeq` (from the
//! byte buffer's perspective: we write bytes into a WireWriter); the
//! C# side calls it the encode phase, matching what the generated
//! code does from the C# programmer's perspective.
//!
//! Most ops return a single statement; a length-prefixed encoded
//! array returns two (the `WriteI32(length)` call and the per-element
//! `foreach`).

use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ops::{WriteOp, WriteSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::super::ast::{
    CSharpExpression, CSharpIdentity, CSharpLiteral, CSharpLocalName, CSharpMethodName,
    CSharpPropertyName, CSharpStatement, CSharpType,
};
use super::value::{Renames, render_value};

/// Counter state for the synthesized C# locals a write expression
/// introduces: the `opt{n}` pattern binding inside a `WriteOp::Option`
/// encode `if` and the `item{n}` loop variable inside a
/// `WriteOp::Vec` encoded foreach. Sibling write statements in one
/// method body share a single instance so their counters advance
/// together and no two declarations collide.
#[derive(Debug, Default)]
pub(crate) struct EncodeLocalCounters {
    option_binding_index: usize,
    loop_var_index: usize,
}

impl EncodeLocalCounters {
    /// Mint the next `opt{n}` pattern-binding local.
    fn next_option_binding(&mut self) -> CSharpLocalName {
        let i = self.option_binding_index;
        self.option_binding_index += 1;
        CSharpLocalName::encode_option_binding(i)
    }

    /// Mint the next `item{n}` loop-variable local.
    fn next_loop_var(&mut self) -> CSharpLocalName {
        let i = self.loop_var_index;
        self.loop_var_index += 1;
        CSharpLocalName::encode_loop_var(i)
    }
}

/// Render the first op of a [`WriteSeq`] as one or more C# statements.
/// `writer` is the receiver expression for the wire-write calls:
/// typically `wire` (a free identifier) at the record level, or the
/// per-param `_wire_{name}` local inside a function's wire-writer block.
pub(crate) fn lower_encode_expr(
    seq: &WriteSeq,
    writer: &CSharpExpression,
    renames: &Renames,
    locals: &mut EncodeLocalCounters,
) -> Vec<CSharpStatement> {
    let op = seq.ops.first().expect("write ops");
    match op {
        WriteOp::Primitive { primitive, value } => {
            vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(writer.clone()),
                method: primitive_write_method(*primitive),
                type_args: vec![],
                args: vec![render_value(value, renames)].into(),
            })]
        }
        WriteOp::String { value } => {
            vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(writer.clone()),
                method: CSharpMethodName::from_source("write_string"),
                type_args: vec![],
                args: vec![render_value(value, renames)].into(),
            })]
        }
        WriteOp::Bytes { value } => {
            vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(writer.clone()),
                method: CSharpMethodName::from_source("write_bytes"),
                type_args: vec![],
                args: vec![render_value(value, renames)].into(),
            })]
        }
        WriteOp::Record { value, .. } => {
            vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(render_value(value, renames)),
                method: CSharpMethodName::from_source("wire_encode_to"),
                type_args: vec![],
                args: vec![writer.clone()].into(),
            })]
        }
        WriteOp::Enum {
            value,
            layout: EnumLayout::CStyle { .. } | EnumLayout::Data { .. },
            ..
        } => vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
            receiver: Box::new(render_value(value, renames)),
            method: CSharpMethodName::from_source("wire_encode_to"),
            type_args: vec![],
            args: vec![writer.clone()].into(),
        })],
        WriteOp::Option { value, some } => {
            let binding = locals.next_option_binding();
            let mut inner_renames = renames.clone();
            // The IR's inner write references `v` as the option's
            // bound payload. Rebind to the pattern variable so
            // nested `WriteString(Var("v"))` renders as
            // `wire.WriteString(opt0)`.
            inner_renames.insert(
                "v".to_string(),
                CSharpExpression::Identity(CSharpIdentity::Local(binding.clone())),
            );
            let inner_stmts = lower_encode_expr(some, writer, &inner_renames, locals);
            let tag_byte = |byte: i64| {
                CSharpStatement::Expression(CSharpExpression::MethodCall {
                    receiver: Box::new(writer.clone()),
                    method: CSharpMethodName::from_source("write_u8"),
                    type_args: vec![],
                    args: vec![CSharpExpression::Cast {
                        target: CSharpType::Byte,
                        inner: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(byte))),
                    }]
                    .into(),
                })
            };
            let mut then = vec![tag_byte(1)];
            then.extend(inner_stmts);
            vec![CSharpStatement::If {
                cond: CSharpExpression::IsBindingPattern {
                    value: Box::new(render_value(value, renames)),
                    binding,
                },
                then,
                otherwise: Some(vec![tag_byte(0)]),
            }]
        }
        WriteOp::Vec {
            value,
            element_type: TypeExpr::Primitive(p),
            layout: VecLayout::Blittable { .. },
            ..
        } => vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
            receiver: Box::new(writer.clone()),
            method: primitive_vec_writer_method(*p),
            type_args: vec![],
            args: vec![render_value(value, renames)].into(),
        })],
        WriteOp::Vec {
            value,
            layout: VecLayout::Blittable { .. },
            ..
        } => {
            // Reached when a record field or enum-variant field
            // carries a `Vec<BlittableRecord>`. The write side infers
            // `T` from the argument's managed type, so no type
            // argument is emitted (unlike the read side, which needs
            // `<T>` to pick the method's return type).
            vec![CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(writer.clone()),
                method: CSharpMethodName::from_source("write_blittable_array"),
                type_args: vec![],
                args: vec![render_value(value, renames)].into(),
            })]
        }
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout: VecLayout::Encoded,
        } => {
            let loop_var = locals.next_loop_var();
            let mut inner_renames = renames.clone();
            // The IR's inner references `item` as the per-element
            // binding; rebind to `item{n}` so nested writes render
            // against the foreach variable.
            inner_renames.insert(
                "item".to_string(),
                CSharpExpression::Identity(CSharpIdentity::Local(loop_var.clone())),
            );
            let inner_stmts = lower_encode_expr(element, writer, &inner_renames, locals);
            let length_stmt = CSharpStatement::Expression(CSharpExpression::MethodCall {
                receiver: Box::new(writer.clone()),
                method: CSharpMethodName::from_source("write_i32"),
                type_args: vec![],
                args: vec![CSharpExpression::MemberAccess {
                    receiver: Box::new(render_value(value, renames)),
                    name: CSharpPropertyName::from_source("length"),
                }]
                .into(),
            });
            let foreach_stmt = CSharpStatement::ForEach {
                elem_type: CSharpType::from_type_expr(element_type),
                var: loop_var,
                collection: render_value(value, renames),
                body: inner_stmts,
            };
            vec![length_stmt, foreach_stmt]
        }
        other => todo!(
            "C# backend has not yet implemented write support for {:?}",
            other
        ),
    }
}

fn primitive_write_method(primitive: PrimitiveType) -> CSharpMethodName {
    match primitive {
        // `WriteNInt` / `WriteNUInt` carry a capital `I`/`U` in the
        // middle of their names; the snake_case splitter would render
        // them `WriteNint` / `WriteNuint`, so wrap the exact names.
        PrimitiveType::ISize => CSharpMethodName::new("WriteNInt"),
        PrimitiveType::USize => CSharpMethodName::new("WriteNUInt"),
        PrimitiveType::Bool => CSharpMethodName::from_source("write_bool"),
        PrimitiveType::I8 => CSharpMethodName::from_source("write_i8"),
        PrimitiveType::U8 => CSharpMethodName::from_source("write_u8"),
        PrimitiveType::I16 => CSharpMethodName::from_source("write_i16"),
        PrimitiveType::U16 => CSharpMethodName::from_source("write_u16"),
        PrimitiveType::I32 => CSharpMethodName::from_source("write_i32"),
        PrimitiveType::U32 => CSharpMethodName::from_source("write_u32"),
        PrimitiveType::I64 => CSharpMethodName::from_source("write_i64"),
        PrimitiveType::U64 => CSharpMethodName::from_source("write_u64"),
        PrimitiveType::F32 => CSharpMethodName::from_source("write_f32"),
        PrimitiveType::F64 => CSharpMethodName::from_source("write_f64"),
    }
}

/// The `WireWriter` method a blittable-primitive `Vec<T>` encode call
/// targets. Bool, isize, and usize have dedicated helpers because the
/// wire shapes of those primitives don't line up with the generic
/// `WriteBlittableArray<T>` path; every other primitive uses the
/// generic method and lets C# infer `T` from the argument.
fn primitive_vec_writer_method(primitive: PrimitiveType) -> CSharpMethodName {
    match primitive {
        // `WriteNIntArray` / `WriteNUIntArray` carry a capital `I`/`U`
        // in the middle of their name; the snake_case splitter would
        // render them `WriteNintArray` / `WriteNuintArray`, so we wrap
        // the pre-formed PascalCase name instead.
        PrimitiveType::ISize => CSharpMethodName::new("WriteNIntArray"),
        PrimitiveType::USize => CSharpMethodName::new("WriteNUIntArray"),
        PrimitiveType::Bool => CSharpMethodName::from_source("write_bool_array"),
        _ => CSharpMethodName::from_source("write_blittable_array"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::codec::VecLayout;
    use crate::ir::ids::{FieldName, ParamName, RecordId};
    use crate::ir::ops::{SizeExpr, ValueExpr, WireShape, WriteOp, WriteSeq};

    fn wire_receiver() -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("wire")))
    }

    fn seq(op: WriteOp) -> WriteSeq {
        WriteSeq {
            size: SizeExpr::Fixed(0),
            ops: vec![op],
            shape: WireShape::Value,
        }
    }

    fn field_of_this(field: &str) -> ValueExpr {
        ValueExpr::Field(Box::new(ValueExpr::Instance), FieldName::new(field))
    }

    fn named(name: &str) -> ValueExpr {
        ValueExpr::Named(ParamName::new(name).as_str().to_string())
    }

    fn lower_fresh(seq: &WriteSeq) -> Vec<CSharpStatement> {
        let mut locals = EncodeLocalCounters::default();
        let renames = Renames::new();
        lower_encode_expr(seq, &wire_receiver(), &renames, &mut locals)
    }

    /// Single-statement assertion shorthand: assert the lowering
    /// produced exactly one statement and check its rendered form.
    #[track_caller]
    fn assert_single(stmts: &[CSharpStatement], expected: &str) {
        assert_eq!(stmts.len(), 1, "expected one statement, got {stmts:?}");
        assert_eq!(stmts[0].to_string(), expected);
    }

    #[test]
    fn primitive_write_renders_typed_method_call() {
        let stmts = lower_fresh(&seq(WriteOp::Primitive {
            primitive: PrimitiveType::F64,
            value: field_of_this("x"),
        }));
        assert_single(&stmts, "wire.WriteF64(this.X)");
    }

    #[test]
    fn pointer_sized_primitive_writes_keep_csharp_helper_casing() {
        let isize = lower_fresh(&seq(WriteOp::Primitive {
            primitive: PrimitiveType::ISize,
            value: ValueExpr::Var("item".to_string()),
        }));
        assert_single(&isize, "wire.WriteNInt(item)");

        let usize = lower_fresh(&seq(WriteOp::Primitive {
            primitive: PrimitiveType::USize,
            value: ValueExpr::Var("item".to_string()),
        }));
        assert_single(&usize, "wire.WriteNUInt(item)");
    }

    #[test]
    fn string_write_renders_write_string_call() {
        let stmts = lower_fresh(&seq(WriteOp::String {
            value: field_of_this("name"),
        }));
        assert_single(&stmts, "wire.WriteString(this.Name)");
    }

    #[test]
    fn record_write_renders_wire_encode_to_on_value() {
        let stmts = lower_fresh(&seq(WriteOp::Record {
            id: RecordId::new("point"),
            value: field_of_this("origin"),
            fields: vec![],
        }));
        assert_single(&stmts, "this.Origin.WireEncodeTo(wire)");
    }

    /// A `WriteOp::Enum` with a C-style layout emits the same call
    /// shape as a record field: `{value}.WireEncodeTo(wire)`. The
    /// extension method on the generated `{Name}Wire` class lets the
    /// enum slot into that uniform shape at no runtime cost.
    #[test]
    fn c_style_enum_write_matches_record_call_shape() {
        use crate::ir::codec::EnumLayout;
        use crate::ir::ids::EnumId;
        use boltffi_ffi_rules::transport::EnumTagStrategy;

        let stmts = lower_fresh(&seq(WriteOp::Enum {
            id: EnumId::new("status"),
            value: field_of_this("status"),
            layout: EnumLayout::CStyle {
                tag_type: PrimitiveType::I32,
                tag_strategy: EnumTagStrategy::OrdinalIndex,
                is_error: false,
            },
        }));
        assert_single(&stmts, "this.Status.WireEncodeTo(wire)");
    }

    #[test]
    fn option_write_renders_if_else_with_tag_bytes_and_pattern_binding() {
        let inner = seq(WriteOp::String {
            value: ValueExpr::Var("v".to_string()),
        });
        let stmts = lower_fresh(&seq(WriteOp::Option {
            value: field_of_this("name"),
            some: Box::new(inner),
        }));
        assert_single(
            &stmts,
            "if (this.Name is { } opt0) { wire.WriteU8((byte)1); wire.WriteString(opt0); } else { wire.WriteU8((byte)0); }",
        );
    }

    /// Two option writes sharing one [`EncodeLocalCounters`] pick up distinct
    /// `opt{n}` names, because a shared `opt0` would redeclare the
    /// same local in one method scope.
    #[test]
    fn sibling_option_writes_use_distinct_pattern_bindings() {
        let mut locals = EncodeLocalCounters::default();
        let renames = Renames::new();
        let first = lower_encode_expr(
            &seq(WriteOp::Option {
                value: field_of_this("a"),
                some: Box::new(seq(WriteOp::String {
                    value: ValueExpr::Var("v".to_string()),
                })),
            }),
            &wire_receiver(),
            &renames,
            &mut locals,
        );
        let second = lower_encode_expr(
            &seq(WriteOp::Option {
                value: field_of_this("b"),
                some: Box::new(seq(WriteOp::String {
                    value: ValueExpr::Var("v".to_string()),
                })),
            }),
            &wire_receiver(),
            &renames,
            &mut locals,
        );
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert!(
            first[0].to_string().contains(" opt0"),
            "expecting first write to bind opt0, got {}",
            first[0]
        );
        assert!(
            second[0].to_string().contains(" opt1"),
            "expecting second write to bind opt1, got {}",
            second[0]
        );
    }

    #[test]
    fn primitive_vec_blittable_uses_generic_write_blittable_array() {
        let stmts = lower_fresh(&seq(WriteOp::Vec {
            value: named("numbers"),
            element_type: TypeExpr::Primitive(PrimitiveType::I32),
            element: Box::new(seq(WriteOp::Primitive {
                primitive: PrimitiveType::I32,
                value: ValueExpr::Var("item".to_string()),
            })),
            layout: VecLayout::Blittable { element_size: 4 },
        }));
        assert_single(&stmts, "wire.WriteBlittableArray(numbers)");
    }

    #[test]
    fn bool_vec_blittable_uses_dedicated_bool_writer() {
        let stmts = lower_fresh(&seq(WriteOp::Vec {
            value: named("flags"),
            element_type: TypeExpr::Primitive(PrimitiveType::Bool),
            element: Box::new(seq(WriteOp::Primitive {
                primitive: PrimitiveType::Bool,
                value: ValueExpr::Var("item".to_string()),
            })),
            layout: VecLayout::Blittable { element_size: 1 },
        }));
        assert_single(&stmts, "wire.WriteBoolArray(flags)");
    }

    #[test]
    fn isize_vec_blittable_keeps_capital_i_in_nint_name() {
        let stmts = lower_fresh(&seq(WriteOp::Vec {
            value: named("offsets"),
            element_type: TypeExpr::Primitive(PrimitiveType::ISize),
            element: Box::new(seq(WriteOp::Primitive {
                primitive: PrimitiveType::ISize,
                value: ValueExpr::Var("item".to_string()),
            })),
            layout: VecLayout::Blittable { element_size: 8 },
        }));
        assert_single(&stmts, "wire.WriteNIntArray(offsets)");
    }

    /// An encoded vec lowers to two top-level statements: a length
    /// prefix and a per-element foreach. The template iterates them
    /// onto separate lines.
    #[test]
    fn encoded_vec_lowers_to_length_write_and_foreach() {
        let stmts = lower_fresh(&seq(WriteOp::Vec {
            value: field_of_this("names"),
            element_type: TypeExpr::String,
            element: Box::new(seq(WriteOp::String {
                value: ValueExpr::Var("item".to_string()),
            })),
            layout: VecLayout::Encoded,
        }));
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0].to_string(), "wire.WriteI32(this.Names.Length)");
        assert_eq!(
            stmts[1].to_string(),
            "foreach (string item0 in this.Names) { wire.WriteString(item0); }"
        );
    }
}
