//! Wire encoding operations for the IR.
//!
//! This module defines the operation sequences that the [`Lowerer`](crate::ir::Lowerer)
//! produces and backends consume. A [`ReadSeq`] describes how to decode a type from
//! a wire buffer, and a [`WriteSeq`] describes how to encode it. Each sequence
//! contains a list of typed operations and a [`SizeExpr`] for computing the byte size.
//!
//! Backends walk these sequences and emit target-language code for each operation.
//! They do not restructure or reinterpret the operations.

use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ids::{BuiltinId, CustomTypeId, EnumId, FieldName, RecordId};
use crate::ir::types::{PrimitiveType, TypeExpr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireShape {
    Value,
    Optional,
    Sequence,
}

/// Represents an access path to a value in generated code.
///
/// [`WriteOp`] operations need to reference the value being encoded. Rather than
/// emitting language-specific strings directly, the [`Lowerer`](crate::ir::Lowerer)
/// builds a tree of [`ValueExpr`] nodes. Each backend renders the tree into its
/// own syntax:
///
/// ```text
/// ValueExpr::Field(ValueExpr::Instance, "name")
///   -> Kotlin: this.name
///   -> Swift:  self.name
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    /// The receiver object. Kotlin renders as `this`, Swift as `self`.
    Instance,
    /// A local variable, such as a loop counter in a vec encoder.
    Var(String),
    /// A parameter name from the function signature.
    Named(String),
    /// A field access on a parent expression.
    Field(Box<ValueExpr>, FieldName),
}

impl ValueExpr {
    pub fn field(&self, name: FieldName) -> ValueExpr {
        ValueExpr::Field(Box::new(self.clone()), name)
    }

    /// Replaces the root of this access path with `new_root`.
    ///
    /// The [`Lowerer`](crate::ir::Lowerer) builds write operations with
    /// [`Instance`](ValueExpr::Instance) as the root. When those operations
    /// get reused in a different context, such as an enum variant arm where
    /// the value comes from a match binding, the root needs to be swapped
    /// without rebuilding the whole tree.
    pub fn remap_root(&self, new_root: ValueExpr) -> ValueExpr {
        match self {
            ValueExpr::Instance | ValueExpr::Var(_) | ValueExpr::Named(_) => new_root,
            ValueExpr::Field(parent, name) => {
                ValueExpr::Field(Box::new(parent.remap_root(new_root)), name.clone())
            }
        }
    }
}

/// Describes how to compute the byte size of an encoded value.
///
/// Some types have a fixed size known at compile time. Others depend on the
/// actual value being encoded, so the backend must emit runtime size calculation
/// code. The [`Lowerer`](crate::ir::Lowerer) picks the right variant during
/// codec planning.
#[derive(Debug, Clone)]
pub enum WireSizeOwner {
    Record(RecordId),
    Enum(EnumId),
}

#[derive(Debug, Clone)]
pub enum SizeExpr {
    /// Statically known byte count. Used for primitives and blittable records.
    Fixed(usize),
    /// Size depends on runtime data that cannot be expressed as a value.
    /// The backend must track wire position before and after to compute it.
    /// Data enums hit this because the size varies per variant.
    Runtime,
    /// 4 bytes for the length prefix plus the UTF-8 byte length of the string.
    StringLen(ValueExpr),
    /// 4 bytes for the length prefix plus the raw byte length.
    BytesLen(ValueExpr),
    /// Size determined by calling a type-specific size method on the value.
    ValueSize(ValueExpr),
    /// Size of a wire-encoded compound value. The backend calls the owning
    /// codec size method when one exists.
    WireSize {
        value: ValueExpr,
        owner: Option<WireSizeOwner>,
    },
    /// Size of a builtin type like Duration, Uuid, or Url.
    BuiltinSize { id: BuiltinId, value: ValueExpr },
    /// Sum of multiple size expressions. Used for records with mixed field types.
    Sum(Vec<SizeExpr>),
    /// 1 byte for the tag, plus the inner size if the value is present.
    OptionSize {
        value: ValueExpr,
        inner: Box<SizeExpr>,
    },
    /// 4 bytes for the element count, plus the total size of all elements.
    /// Blittable vecs use element_count * element_size directly.
    VecSize {
        value: ValueExpr,
        inner: Box<SizeExpr>,
        layout: VecLayout,
    },
    /// 1 byte for the ok/err tag, plus the size of whichever branch is present.
    ResultSize {
        value: ValueExpr,
        ok: Box<SizeExpr>,
        err: Box<SizeExpr>,
    },
}

/// A sequence of [`ReadOp`] operations that decode one value from a wire buffer.
///
/// Contains the operations to execute in order, a [`SizeExpr`] for the byte
/// size of the encoded value, and a [`WireShape`] indicating whether the
/// result is a single value, an optional, or a list.
#[derive(Debug, Clone)]
pub struct ReadSeq {
    pub size: SizeExpr,
    pub ops: Vec<ReadOp>,
    // needed because both [`Vec`] and [`Option`] produce nested [`ReadSeq`], but
    // backends wrap them differently. the ops alone do not distinguish.
    pub shape: WireShape,
}

/// A sequence of [`WriteOp`] operations that encode one value into a wire buffer.
///
/// Mirrors [`ReadSeq`], but each operation carries a [`ValueExpr`] identifying
/// the source value.
#[derive(Debug, Clone)]
pub struct WriteSeq {
    pub size: SizeExpr,
    pub ops: Vec<WriteOp>,
    pub shape: WireShape,
}

#[derive(Debug, Clone)]
pub enum OffsetExpr {
    Fixed(usize),
    Base,
    BasePlus(usize),
    Var(String),
    VarPlus(String, usize),
}

#[derive(Debug, Clone)]
pub enum ReadOp {
    Primitive {
        primitive: PrimitiveType,
        offset: OffsetExpr,
    },
    String {
        offset: OffsetExpr,
    },
    Option {
        tag_offset: OffsetExpr,
        some: Box<ReadSeq>,
    },
    Vec {
        len_offset: OffsetExpr,
        element_type: TypeExpr,
        element: Box<ReadSeq>,
        layout: VecLayout,
    },
    Record {
        id: RecordId,
        offset: OffsetExpr,
        fields: Vec<FieldReadOp>,
    },
    Enum {
        id: EnumId,
        offset: OffsetExpr,
        layout: EnumLayout,
    },
    Result {
        tag_offset: OffsetExpr,
        ok: Box<ReadSeq>,
        err: Box<ReadSeq>,
    },
    Builtin {
        id: BuiltinId,
        offset: OffsetExpr,
    },
    Custom {
        id: CustomTypeId,
        underlying: Box<ReadSeq>,
    },
}

#[derive(Debug, Clone)]
pub enum WriteOp {
    Primitive {
        primitive: PrimitiveType,
        value: ValueExpr,
    },
    String {
        value: ValueExpr,
    },
    Option {
        value: ValueExpr,
        some: Box<WriteSeq>,
    },
    Vec {
        value: ValueExpr,
        element_type: TypeExpr,
        element: Box<WriteSeq>,
        layout: VecLayout,
    },
    Record {
        id: RecordId,
        value: ValueExpr,
        fields: Vec<FieldWriteOp>,
    },
    Enum {
        id: EnumId,
        value: ValueExpr,
        layout: EnumLayout,
    },
    Result {
        value: ValueExpr,
        ok: Box<WriteSeq>,
        err: Box<WriteSeq>,
    },
    Builtin {
        id: BuiltinId,
        value: ValueExpr,
    },
    Custom {
        id: CustomTypeId,
        value: ValueExpr,
        underlying: Box<WriteSeq>,
    },
}

#[derive(Debug, Clone)]
pub struct FieldReadOp {
    pub name: FieldName,
    pub seq: ReadSeq,
}

#[derive(Debug, Clone)]
pub struct FieldWriteOp {
    pub name: FieldName,
    pub accessor: ValueExpr,
    pub seq: WriteSeq,
}

/// Rewrites every [`ValueExpr`] in a [`WriteSeq`] to use a different root.
///
/// The [`Lowerer`](crate::ir::Lowerer) builds write ops once per type with
/// [`Instance`](ValueExpr::Instance) as root, then remaps when reusing those
/// ops in a different context, such as inside an enum variant encoder or a
/// callback parameter.
pub fn remap_root_in_seq(seq: &WriteSeq, new_root: ValueExpr) -> WriteSeq {
    WriteSeq {
        size: remap_root_in_size(&seq.size, &new_root),
        ops: seq
            .ops
            .iter()
            .map(|op| remap_root_in_op(op, &new_root))
            .collect(),
        shape: seq.shape,
    }
}

fn remap_root_in_size(size: &SizeExpr, new_root: &ValueExpr) -> SizeExpr {
    match size {
        SizeExpr::Fixed(value) => SizeExpr::Fixed(*value),
        SizeExpr::Runtime => SizeExpr::Runtime,
        SizeExpr::StringLen(value) => SizeExpr::StringLen(value.remap_root(new_root.clone())),
        SizeExpr::BytesLen(value) => SizeExpr::BytesLen(value.remap_root(new_root.clone())),
        SizeExpr::ValueSize(value) => SizeExpr::ValueSize(value.remap_root(new_root.clone())),
        SizeExpr::WireSize { value, owner } => SizeExpr::WireSize {
            value: value.remap_root(new_root.clone()),
            owner: owner.clone(),
        },
        SizeExpr::BuiltinSize { id, value } => SizeExpr::BuiltinSize {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
        },
        SizeExpr::Sum(parts) => SizeExpr::Sum(
            parts
                .iter()
                .map(|part| remap_root_in_size(part, new_root))
                .collect(),
        ),
        SizeExpr::OptionSize { value, inner } => SizeExpr::OptionSize {
            value: value.remap_root(new_root.clone()),
            inner: inner.clone(),
        },
        SizeExpr::VecSize {
            value,
            inner,
            layout,
        } => SizeExpr::VecSize {
            value: value.remap_root(new_root.clone()),
            inner: inner.clone(),
            layout: layout.clone(),
        },
        SizeExpr::ResultSize { value, ok, err } => SizeExpr::ResultSize {
            value: value.remap_root(new_root.clone()),
            ok: ok.clone(),
            err: err.clone(),
        },
    }
}

fn remap_root_in_op(op: &WriteOp, new_root: &ValueExpr) -> WriteOp {
    match op {
        WriteOp::Primitive { primitive, value } => WriteOp::Primitive {
            primitive: *primitive,
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::String { value } => WriteOp::String {
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::Option { value, some } => WriteOp::Option {
            value: value.remap_root(new_root.clone()),
            some: some.clone(),
        },
        WriteOp::Vec {
            value,
            element_type,
            element,
            layout,
        } => WriteOp::Vec {
            value: value.remap_root(new_root.clone()),
            element_type: element_type.clone(),
            element: element.clone(),
            layout: layout.clone(),
        },
        WriteOp::Record { id, value, fields } => WriteOp::Record {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
            fields: fields
                .iter()
                .map(|field| FieldWriteOp {
                    name: field.name.clone(),
                    accessor: field.accessor.remap_root(new_root.clone()),
                    seq: remap_root_in_seq(&field.seq, new_root.clone()),
                })
                .collect(),
        },
        WriteOp::Enum { id, value, layout } => WriteOp::Enum {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
            layout: layout.clone(),
        },
        WriteOp::Result { value, ok, err } => WriteOp::Result {
            value: value.remap_root(new_root.clone()),
            ok: ok.clone(),
            err: err.clone(),
        },
        WriteOp::Builtin { id, value } => WriteOp::Builtin {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
        },
        WriteOp::Custom {
            id,
            value,
            underlying,
        } => WriteOp::Custom {
            id: id.clone(),
            value: value.remap_root(new_root.clone()),
            underlying: underlying.clone(),
        },
    }
}
