//! Translates an IR [`ReadSeq`] into a typed C# decode-phase
//! expression tree. Companion to [`super::size`] and [`super::encode`].
//!
//! The walker carries a `reader` receiver expression so each
//! `ReadOp` that needs a stateful read (`reader.ReadX()`) can target
//! it. Top-level callers pass `Ident::Local(CSharpLocalName::new("reader"))`;
//! the encoded-vec branch introduces a `Lambda` whose body uses the
//! closure parameter local as the receiver.
//!
//! Closure-variable numbering is outer-first to align with the size
//! and encode arcs: the outermost lambda gets `r0`, nested lambdas
//! advance the counter outward-in.
//!
//! Top-level vec returns are dispatched in [`super::functions`]
//! (different wire shape: no length prefix). The walker handles only
//! nested vecs.

use std::collections::HashSet;

use crate::ir::codec::{EnumLayout, VecLayout};
use crate::ir::ops::{ReadOp, ReadSeq};
use crate::ir::types::{PrimitiveType, TypeExpr};

use super::super::ast::{
    CSharpArgumentList, CSharpBinaryOp, CSharpClassName, CSharpExpression, CSharpIdentity,
    CSharpLiteral, CSharpLocalName, CSharpMethodName, CSharpNamespace, CSharpType,
    CSharpTypeReference,
};

/// Counter state for synthesized C# locals introduced by the decode
/// walker: today only the `r{n}` closure parameter inside the
/// `ReadEncodedArray<T>(rN => ...)` lambda. Sibling decode trees in
/// one method body share a single instance so their counters advance
/// together and no two declarations collide in that scope.
#[derive(Debug, Default)]
pub(crate) struct DecodeLocalCounters {
    closure_var_index: usize,
}

impl DecodeLocalCounters {
    pub(crate) fn next_closure_var(&mut self) -> CSharpLocalName {
        let i = self.closure_var_index;
        self.closure_var_index += 1;
        CSharpLocalName::decode_closure_var(i)
    }
}

/// Render the first op of a [`ReadSeq`] as a typed C# decode
/// expression.
///
/// `reader` is the receiver expression for stateful reads: at the
/// top level a free `reader` ident; inside the encoded-array
/// closure body it's the closure parameter binding (e.g. `r0`).
pub(crate) fn lower_decode_expr(
    seq: &ReadSeq,
    reader: &CSharpExpression,
    shadowed: Option<&HashSet<CSharpClassName>>,
    namespace: &CSharpNamespace,
    locals: &mut DecodeLocalCounters,
) -> CSharpExpression {
    let op = seq.ops.first().expect("read ops");
    match op {
        ReadOp::Primitive { primitive, .. } => CSharpExpression::MethodCall {
            receiver: Box::new(reader.clone()),
            method: primitive_read_method(*primitive),
            type_args: vec![],
            args: CSharpArgumentList::default(),
        },
        ReadOp::String { .. } => CSharpExpression::MethodCall {
            receiver: Box::new(reader.clone()),
            method: CSharpMethodName::from_source("read_string"),
            type_args: vec![],
            args: CSharpArgumentList::default(),
        },
        ReadOp::Bytes { .. } => CSharpExpression::MethodCall {
            receiver: Box::new(reader.clone()),
            method: CSharpMethodName::from_source("read_bytes"),
            type_args: vec![],
            args: CSharpArgumentList::default(),
        },
        ReadOp::Record { id, .. } => {
            let class_name: CSharpClassName = id.into();
            let type_ref =
                CSharpTypeReference::Plain(class_name).qualify_if_shadowed_opt(shadowed, namespace);
            CSharpExpression::MethodCall {
                receiver: Box::new(CSharpExpression::TypeRef(type_ref)),
                method: CSharpMethodName::from_source("decode"),
                type_args: vec![],
                args: vec![reader.clone()].into(),
            }
        }
        ReadOp::Enum {
            id,
            layout: EnumLayout::CStyle { .. },
            ..
        } => {
            // {Name}Wire.Decode(reader); the Wire suffix is unambiguous
            // against same-name nested variants, so no shadow-qualify.
            let base: CSharpClassName = id.into();
            let wire = CSharpClassName::wire_helper(&base);
            CSharpExpression::MethodCall {
                receiver: Box::new(CSharpExpression::TypeRef(CSharpTypeReference::Plain(wire))),
                method: CSharpMethodName::from_source("decode"),
                type_args: vec![],
                args: vec![reader.clone()].into(),
            }
        }
        ReadOp::Enum {
            id,
            layout: EnumLayout::Data { .. },
            ..
        } => {
            let class_name: CSharpClassName = id.into();
            let type_ref =
                CSharpTypeReference::Plain(class_name).qualify_if_shadowed_opt(shadowed, namespace);
            CSharpExpression::MethodCall {
                receiver: Box::new(CSharpExpression::TypeRef(type_ref)),
                method: CSharpMethodName::from_source("decode"),
                type_args: vec![],
                args: vec![reader.clone()].into(),
            }
        }
        ReadOp::Option { some, .. } => {
            let inner = lower_decode_expr(some, reader, shadowed, namespace, locals);
            let inner_ty =
                CSharpType::from_read_op(some.ops.first().expect("option inner read op"))
                    .qualify_if_shadowed_opt(shadowed, namespace);
            // reader.ReadU8() == 0 ? (Inner?)null : <inner>
            CSharpExpression::Ternary {
                cond: Box::new(CSharpExpression::Binary {
                    op: CSharpBinaryOp::Eq,
                    left: Box::new(CSharpExpression::MethodCall {
                        receiver: Box::new(reader.clone()),
                        method: CSharpMethodName::from_source("read_u8"),
                        type_args: vec![],
                        args: CSharpArgumentList::default(),
                    }),
                    right: Box::new(CSharpExpression::Literal(CSharpLiteral::Int(0))),
                }),
                then: Box::new(CSharpExpression::Cast {
                    target: CSharpType::Nullable(Box::new(inner_ty)),
                    inner: Box::new(CSharpExpression::Literal(CSharpLiteral::Null)),
                }),
                otherwise: Box::new(inner),
            }
        }
        ReadOp::Vec {
            element_type: TypeExpr::Primitive(p),
            layout: VecLayout::Blittable { .. },
            ..
        } => {
            // Reached only for nested vecs (top-level Vec returns
            // dispatch in `super::functions::return_kind`). Nested
            // means length-prefixed.
            CSharpExpression::MethodCall {
                receiver: Box::new(reader.clone()),
                method: nested_blittable_primitive_array_method(*p),
                type_args: nested_blittable_primitive_array_type_args(*p),
                args: CSharpArgumentList::default(),
            }
        }
        ReadOp::Vec {
            element_type,
            layout: VecLayout::Blittable { .. },
            ..
        } => {
            // Nested Vec<BlittableRecord> field/variant slot.
            let element_ty = CSharpType::from_type_expr(element_type)
                .qualify_if_shadowed_opt(shadowed, namespace);
            CSharpExpression::MethodCall {
                receiver: Box::new(reader.clone()),
                method: CSharpMethodName::from_source("read_length_prefixed_blittable_array"),
                type_args: vec![element_ty],
                args: CSharpArgumentList::default(),
            }
        }
        ReadOp::Vec {
            element_type,
            element,
            layout: VecLayout::Encoded,
            ..
        } => {
            // Outer-first numbering: the lambda we're producing now
            // takes the next counter, then the recursive body uses it
            // as its receiver.
            let closure_var = locals.next_closure_var();
            let closure_receiver =
                CSharpExpression::Identity(CSharpIdentity::Local(closure_var.clone()));
            let inner = lower_decode_expr(element, &closure_receiver, shadowed, namespace, locals);
            let element_ty = CSharpType::from_type_expr(element_type)
                .qualify_if_shadowed_opt(shadowed, namespace);
            CSharpExpression::MethodCall {
                receiver: Box::new(reader.clone()),
                method: CSharpMethodName::from_source("read_encoded_array"),
                type_args: vec![element_ty],
                args: vec![CSharpExpression::Lambda {
                    param: closure_var,
                    body: Box::new(inner),
                }]
                .into(),
            }
        }
        other => todo!(
            "C# backend has not yet implemented decode support for {:?}",
            other
        ),
    }
}

/// The reader method for a top-level blittable primitive `Vec<T>`
/// return (no length prefix; the count comes from `FfiBuf.len`).
/// Bool, isize, and usize have dedicated methods. Used by the
/// return-kind classifier in [`super::functions::return_kind`].
pub(crate) fn top_level_blittable_primitive_array_method(
    primitive: PrimitiveType,
) -> CSharpMethodName {
    match primitive {
        PrimitiveType::Bool => CSharpMethodName::from_source("read_bool_array"),
        PrimitiveType::ISize => CSharpMethodName::new("ReadNIntArray"),
        PrimitiveType::USize => CSharpMethodName::new("ReadNUIntArray"),
        _ => CSharpMethodName::from_source("read_blittable_array"),
    }
}

/// The type argument for a top-level blittable primitive `Vec<T>`
/// return: present only on the generic `ReadBlittableArray<T>` path;
/// the dedicated bool/nint/nuint methods carry no type argument.
pub(crate) fn top_level_blittable_primitive_array_type_arg(
    primitive: PrimitiveType,
) -> Option<CSharpType> {
    match primitive {
        PrimitiveType::Bool | PrimitiveType::ISize | PrimitiveType::USize => None,
        other => Some(CSharpType::from(other)),
    }
}

fn primitive_read_method(primitive: PrimitiveType) -> CSharpMethodName {
    match primitive {
        // `ReadNInt` / `ReadNUInt` carry a capital `I`/`U` mid-name
        // that the snake_case splitter would lower-case; wrap the
        // pre-formed name instead.
        PrimitiveType::ISize => CSharpMethodName::new("ReadNInt"),
        PrimitiveType::USize => CSharpMethodName::new("ReadNUInt"),
        PrimitiveType::Bool => CSharpMethodName::from_source("read_bool"),
        PrimitiveType::I8 => CSharpMethodName::from_source("read_i8"),
        PrimitiveType::U8 => CSharpMethodName::from_source("read_u8"),
        PrimitiveType::I16 => CSharpMethodName::from_source("read_i16"),
        PrimitiveType::U16 => CSharpMethodName::from_source("read_u16"),
        PrimitiveType::I32 => CSharpMethodName::from_source("read_i32"),
        PrimitiveType::U32 => CSharpMethodName::from_source("read_u32"),
        PrimitiveType::I64 => CSharpMethodName::from_source("read_i64"),
        PrimitiveType::U64 => CSharpMethodName::from_source("read_u64"),
        PrimitiveType::F32 => CSharpMethodName::from_source("read_f32"),
        PrimitiveType::F64 => CSharpMethodName::from_source("read_f64"),
    }
}

/// The reader method for a nested blittable primitive `Vec<T>`
/// (length-prefixed). Bool, isize, and usize have dedicated methods.
fn nested_blittable_primitive_array_method(primitive: PrimitiveType) -> CSharpMethodName {
    match primitive {
        PrimitiveType::Bool => CSharpMethodName::from_source("read_length_prefixed_bool_array"),
        PrimitiveType::ISize => CSharpMethodName::new("ReadLengthPrefixedNIntArray"),
        PrimitiveType::USize => CSharpMethodName::new("ReadLengthPrefixedNUIntArray"),
        _ => CSharpMethodName::from_source("read_length_prefixed_blittable_array"),
    }
}

fn nested_blittable_primitive_array_type_args(primitive: PrimitiveType) -> Vec<CSharpType> {
    match primitive {
        PrimitiveType::Bool | PrimitiveType::ISize | PrimitiveType::USize => vec![],
        other => vec![CSharpType::from(other)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::codec::{EnumLayout, VecLayout};
    use crate::ir::ids::{EnumId, RecordId};
    use crate::ir::ops::{OffsetExpr, ReadOp, ReadSeq, SizeExpr, WireShape};
    use boltffi_ffi_rules::transport::EnumTagStrategy;

    fn ns() -> CSharpNamespace {
        CSharpNamespace::from_source("demo")
    }

    fn reader() -> CSharpExpression {
        CSharpExpression::Identity(CSharpIdentity::Local(CSharpLocalName::new("reader")))
    }

    fn seq(op: ReadOp) -> ReadSeq {
        ReadSeq {
            size: SizeExpr::Fixed(0),
            ops: vec![op],
            shape: WireShape::Value,
        }
    }

    fn lower_fresh(s: &ReadSeq) -> CSharpExpression {
        let mut locals = DecodeLocalCounters::default();
        lower_decode_expr(s, &reader(), None, &ns(), &mut locals)
    }

    #[test]
    fn primitive_renders_typed_method_call() {
        let r = lower_fresh(&seq(ReadOp::Primitive {
            primitive: PrimitiveType::F64,
            offset: OffsetExpr::Base,
        }));
        assert_eq!(r.to_string(), "reader.ReadF64()");
    }

    #[test]
    fn isize_primitive_keeps_capital_i_in_nint_method() {
        let r = lower_fresh(&seq(ReadOp::Primitive {
            primitive: PrimitiveType::ISize,
            offset: OffsetExpr::Base,
        }));
        assert_eq!(r.to_string(), "reader.ReadNInt()");
    }

    #[test]
    fn string_renders_read_string_call() {
        let r = lower_fresh(&seq(ReadOp::String {
            offset: OffsetExpr::Base,
        }));
        assert_eq!(r.to_string(), "reader.ReadString()");
    }

    #[test]
    fn record_renders_decode_static_call_on_class() {
        let r = lower_fresh(&seq(ReadOp::Record {
            id: RecordId::new("point"),
            offset: OffsetExpr::Base,
            fields: vec![],
        }));
        assert_eq!(r.to_string(), "Point.Decode(reader)");
    }

    #[test]
    fn c_style_enum_renders_wire_helper_decode() {
        let r = lower_fresh(&seq(ReadOp::Enum {
            id: EnumId::new("status"),
            offset: OffsetExpr::Base,
            layout: EnumLayout::CStyle {
                tag_type: PrimitiveType::I32,
                tag_strategy: EnumTagStrategy::Discriminant,
                is_error: false,
            },
        }));
        assert_eq!(r.to_string(), "StatusWire.Decode(reader)");
    }

    /// Inside a data-enum body, a record reference whose class name
    /// collides with a sibling variant must qualify through the module
    /// namespace; otherwise `Point.Decode(reader)` resolves to the
    /// nested `sealed record Point()` (which has no Decode method).
    #[test]
    fn record_qualifies_when_shadowed_by_sibling_variant() {
        let mut locals = DecodeLocalCounters::default();
        let shadowed: HashSet<CSharpClassName> = [CSharpClassName::from_source("point")]
            .into_iter()
            .collect();
        let r = lower_decode_expr(
            &seq(ReadOp::Record {
                id: RecordId::new("point"),
                offset: OffsetExpr::Base,
                fields: vec![],
            }),
            &reader(),
            Some(&shadowed),
            &ns(),
            &mut locals,
        );
        assert_eq!(r.to_string(), "global::Demo.Point.Decode(reader)");
    }

    /// The shadow pass is inert when the class name is not in the shadow
    /// set. Record decodes stay unqualified.
    #[test]
    fn record_leaves_unqualified_when_not_shadowed() {
        let mut locals = DecodeLocalCounters::default();
        let shadowed: HashSet<CSharpClassName> = [CSharpClassName::from_source("circle")]
            .into_iter()
            .collect();
        let r = lower_decode_expr(
            &seq(ReadOp::Record {
                id: RecordId::new("point"),
                offset: OffsetExpr::Base,
                fields: vec![],
            }),
            &reader(),
            Some(&shadowed),
            &ns(),
            &mut locals,
        );
        assert_eq!(r.to_string(), "Point.Decode(reader)");
    }

    #[test]
    fn data_enum_renders_decode_static_call_on_class() {
        let r = lower_fresh(&seq(ReadOp::Enum {
            id: EnumId::new("shape"),
            offset: OffsetExpr::Base,
            layout: EnumLayout::Data {
                tag_type: PrimitiveType::I32,
                tag_strategy: EnumTagStrategy::Discriminant,
                variants: vec![],
            },
        }));
        assert_eq!(r.to_string(), "Shape.Decode(reader)");
    }

    #[test]
    fn option_primitive_renders_pattern_ternary_with_cast_null_branch() {
        let r = lower_fresh(&seq(ReadOp::Option {
            tag_offset: OffsetExpr::Base,
            some: Box::new(seq(ReadOp::Primitive {
                primitive: PrimitiveType::I32,
                offset: OffsetExpr::Base,
            })),
        }));
        assert_eq!(
            r.to_string(),
            "reader.ReadU8() == 0 ? (int?)null : reader.ReadI32()"
        );
    }

    #[test]
    fn nested_blittable_primitive_vec_uses_length_prefixed_method() {
        let r = lower_fresh(&seq(ReadOp::Vec {
            len_offset: OffsetExpr::Base,
            element_type: TypeExpr::Primitive(PrimitiveType::I32),
            element: Box::new(seq(ReadOp::Primitive {
                primitive: PrimitiveType::I32,
                offset: OffsetExpr::Base,
            })),
            layout: VecLayout::Blittable { element_size: 4 },
        }));
        assert_eq!(
            r.to_string(),
            "reader.ReadLengthPrefixedBlittableArray<int>()"
        );
    }

    #[test]
    fn nested_bool_vec_uses_dedicated_length_prefixed_bool_method() {
        let r = lower_fresh(&seq(ReadOp::Vec {
            len_offset: OffsetExpr::Base,
            element_type: TypeExpr::Primitive(PrimitiveType::Bool),
            element: Box::new(seq(ReadOp::Primitive {
                primitive: PrimitiveType::Bool,
                offset: OffsetExpr::Base,
            })),
            layout: VecLayout::Blittable { element_size: 1 },
        }));
        assert_eq!(r.to_string(), "reader.ReadLengthPrefixedBoolArray()");
    }

    #[test]
    fn encoded_vec_renders_method_call_with_lambda_arg() {
        let r = lower_fresh(&seq(ReadOp::Vec {
            len_offset: OffsetExpr::Base,
            element_type: TypeExpr::String,
            element: Box::new(seq(ReadOp::String {
                offset: OffsetExpr::Base,
            })),
            layout: VecLayout::Encoded,
        }));
        assert_eq!(
            r.to_string(),
            "reader.ReadEncodedArray<string>(r0 => r0.ReadString())"
        );
    }

    /// Outer-first numbering: the outer `ReadEncodedArray` lambda binds
    /// `r0`; the inner one binds `r1`. Aligns with the size and encode
    /// arcs.
    #[test]
    fn nested_encoded_vec_assigns_outer_first_closure_numbering() {
        let r = lower_fresh(&seq(ReadOp::Vec {
            len_offset: OffsetExpr::Base,
            element_type: TypeExpr::Vec(Box::new(TypeExpr::String)),
            element: Box::new(seq(ReadOp::Vec {
                len_offset: OffsetExpr::Base,
                element_type: TypeExpr::String,
                element: Box::new(seq(ReadOp::String {
                    offset: OffsetExpr::Base,
                })),
                layout: VecLayout::Encoded,
            })),
            layout: VecLayout::Encoded,
        }));
        let s = r.to_string();
        assert!(
            s.contains("r0 => r0.ReadEncodedArray"),
            "expecting outer to bind r0, got {s}"
        );
        assert!(
            s.contains("r1 => r1.ReadString()"),
            "expecting inner to bind r1, got {s}"
        );
    }
}
