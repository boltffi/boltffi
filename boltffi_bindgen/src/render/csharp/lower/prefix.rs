use crate::ir::ids::FieldName;
use crate::ir::ops::{FieldWriteOp, SizeExpr, ValueExpr, WriteOp, WriteSeq};

use super::lowerer::CSharpLowerer;

impl<'a> CSharpLowerer<'a> {
    /// Rewrites a [`WriteSeq`] so every reference to the encoded value's
    /// instance resolves to `{binding}` instead of the default `this`.
    /// Used for data enum variant fields, where the switch statement
    /// binds each variant as `case Circle _v:` and field references must
    /// go through `_v.Radius` rather than `this.Radius`.
    pub(super) fn prefix_write_seq(seq: &WriteSeq, binding: &str) -> WriteSeq {
        WriteSeq {
            size: Self::prefix_size_expr(&seq.size, binding),
            ops: seq
                .ops
                .iter()
                .map(|op| Self::prefix_write_op(op, binding))
                .collect(),
            shape: seq.shape,
        }
    }

    /// Applies the variant binding rewrite to one [`WriteOp`]. Inner
    /// sequences for loop bodies (`Vec::element`, `Option::some`) are
    /// intentionally not rewritten; they reference loop-bound names, not
    /// the enclosing variant binding.
    fn prefix_write_op(op: &WriteOp, binding: &str) -> WriteOp {
        match op {
            WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
                primitive: *primitive,
                value: Self::prefix_value(value, binding),
            },
            WriteOp::String { value } => WriteOp::String {
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Bytes { value } => WriteOp::Bytes {
                value: Self::prefix_value(value, binding),
            },
            WriteOp::Record { id, value, fields } => WriteOp::Record {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                fields: fields
                    .iter()
                    .map(|f| FieldWriteOp {
                        name: f.name.clone(),
                        accessor: Self::prefix_value(&f.accessor, binding),
                        seq: Self::prefix_write_seq(&f.seq, binding),
                    })
                    .collect(),
            },
            WriteOp::Enum { id, value, layout } => WriteOp::Enum {
                id: id.clone(),
                value: Self::prefix_value(value, binding),
                layout: layout.clone(),
            },
            WriteOp::Vec {
                value,
                element_type,
                element,
                layout,
            } => WriteOp::Vec {
                value: Self::prefix_value(value, binding),
                element_type: element_type.clone(),
                // `element` references the per-iteration loop binding
                // (`item`), which belongs to the foreach the Vec writer
                // emits around the enclosing variant scope. Rewriting it
                // to `_v.item` would break the generated loop; leave the
                // element seq untouched.
                element: element.clone(),
                layout: layout.clone(),
            },
            WriteOp::Option { value, some } => WriteOp::Option {
                value: Self::prefix_value(value, binding),
                // `some` is written inside an `if (field is { } v)` block
                // where inner ops reference `v`, not the outer variant
                // binding. Clone as-is, same as `Vec::element`.
                some: some.clone(),
            },
            other => panic!(
                "prefix_write_op: unsupported op for C# variant fields: {:?}",
                other
            ),
        }
    }

    /// Rewrites `Instance` references to `Var(binding)` and bare
    /// `Named(field)` references to `Field(Var(binding), field)`.
    /// Recursive `Field` references walk into their parent. `Var` is
    /// unchanged.
    fn prefix_value(value: &ValueExpr, binding: &str) -> ValueExpr {
        match value {
            ValueExpr::Instance => ValueExpr::Var(binding.to_string()),
            ValueExpr::Named(name) => ValueExpr::Field(
                Box::new(ValueExpr::Var(binding.to_string())),
                FieldName::new(name),
            ),
            ValueExpr::Var(_) => value.clone(),
            ValueExpr::Field(parent, field) => {
                ValueExpr::Field(Box::new(Self::prefix_value(parent, binding)), field.clone())
            }
        }
    }

    /// Applies the variant binding rewrite to a [`SizeExpr`]. Mirrors
    /// [`Self::prefix_write_op`]'s carve-out for inner loop bodies
    /// (`Vec::inner`, `Option::inner`).
    fn prefix_size_expr(expr: &SizeExpr, binding: &str) -> SizeExpr {
        match expr {
            SizeExpr::Fixed(_) | SizeExpr::Runtime => expr.clone(),
            SizeExpr::StringLen(v) => SizeExpr::StringLen(Self::prefix_value(v, binding)),
            SizeExpr::BytesLen(v) => SizeExpr::BytesLen(Self::prefix_value(v, binding)),
            SizeExpr::ValueSize(v) => SizeExpr::ValueSize(Self::prefix_value(v, binding)),
            SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
                value: Self::prefix_value(value, binding),
                owner: owner.clone(),
            },
            SizeExpr::Sum(parts) => SizeExpr::Sum(
                parts
                    .iter()
                    .map(|p| Self::prefix_size_expr(p, binding))
                    .collect(),
            ),
            SizeExpr::VecSize {
                value,
                inner,
                layout,
            } => SizeExpr::VecSize {
                value: Self::prefix_value(value, binding),
                // `inner` uses the per-element loop variable (`item`) the
                // encoded-array size lambda binds. The `_v` rewrite only
                // applies to the enclosing variant field reference.
                inner: inner.clone(),
                layout: layout.clone(),
            },
            SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
                value: Self::prefix_value(value, binding),
                // `inner` references the unwrapped-option binding `v` that
                // the size-option emit lambda introduces, not the enclosing
                // variant field. Clone as-is, same as `VecSize::inner`.
                inner: inner.clone(),
            },
            other => panic!(
                "prefix_size_expr: unsupported expr for C# variant fields: {:?}",
                other
            ),
        }
    }
}
