use boltffi_binding::{DirectVectorElementType, Primitive, Receive, RecordDecl, Wasm32};

use crate::core::{Error, RenderContext, Result};

use super::super::{
    name_style::Name,
    syntax::{ArgumentList, Expression, Identifier, StringLiteral, TypeName},
};
use super::Type;

pub struct DirectVector {
    kind: VectorKind,
    receive: Option<Receive>,
}

enum VectorKind {
    Primitive(PrimitiveVector),
    Record(RecordVector),
}

struct RecordVector {
    ty: TypeName,
    codec: Identifier,
    size: u64,
    alignment: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PrimitiveVector {
    Bool,
    I8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl DirectVector {
    pub fn new(
        element: &DirectVectorElementType,
        receive: Receive,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Ok(Self {
                kind: VectorKind::Primitive(PrimitiveVector::new(primitive.primitive())?),
                receive: Some(receive),
            }),
            DirectVectorElementType::Record(id) => Ok(Self {
                kind: VectorKind::Record(RecordVector::new(*id, context)?),
                receive: Some(receive),
            }),
            _ => Self::unsupported("unknown direct vector"),
        }
    }

    pub fn outgoing(
        element: &DirectVectorElementType,
        context: &RenderContext<Wasm32>,
    ) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Ok(Self {
                kind: VectorKind::Primitive(PrimitiveVector::new(primitive.primitive())?),
                receive: None,
            }),
            DirectVectorElementType::Record(id) => Ok(Self {
                kind: VectorKind::Record(RecordVector::new(*id, context)?),
                receive: None,
            }),
            _ => Self::unsupported("unknown direct vector"),
        }
    }

    pub fn parameter_type(&self) -> Result<TypeName> {
        match (&self.kind, self.receive) {
            (VectorKind::Primitive(kind), Some(Receive::ByValue | Receive::ByRef))
                if matches!(kind, PrimitiveVector::Bool) =>
            {
                Ok(TypeName::readonly_array(Type::primitive(kind.primitive())?))
            }
            (VectorKind::Primitive(kind), Some(Receive::ByValue | Receive::ByRef)) => {
                Ok(TypeName::union(
                    TypeName::readonly_array(Type::primitive(kind.primitive())?),
                    kind.typed_array(),
                ))
            }
            (VectorKind::Primitive(kind), Some(Receive::ByMutRef))
                if !matches!(kind, PrimitiveVector::Bool) =>
            {
                Ok(kind.typed_array())
            }
            (VectorKind::Primitive(_), Some(Receive::ByMutRef)) => {
                Self::unsupported("mutable boolean slice")
            }
            (VectorKind::Record(record), Some(Receive::ByValue | Receive::ByRef)) => {
                Ok(TypeName::readonly_array(record.ty.clone()))
            }
            (VectorKind::Record(_), Some(Receive::ByMutRef)) => {
                Self::unsupported("mutable direct record vector")
            }
            (_, None) => Ok(self.return_type()),
            _ => Self::unsupported("unknown direct vector receive mode"),
        }
    }

    pub fn return_type(&self) -> TypeName {
        match &self.kind {
            VectorKind::Primitive(PrimitiveVector::Bool) => TypeName::array(TypeName::boolean()),
            VectorKind::Primitive(kind) => kind.typed_array(),
            VectorKind::Record(record) => TypeName::array(record.ty.clone()),
        }
    }

    pub fn allocation(&self, value: Expression) -> Expression {
        match &self.kind {
            VectorKind::Primitive(kind) => Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known(kind.allocation_method()),
                [value].into_iter().collect(),
            ),
            VectorKind::Record(record) => record.allocation(value),
        }
    }

    pub fn take(&self) -> Expression {
        match &self.kind {
            VectorKind::Primitive(kind) => Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known(kind.take_method()),
                ArgumentList::default(),
            ),
            VectorKind::Record(record) => record.take(),
        }
    }

    pub fn writeback(&self) -> bool {
        matches!(self.receive, Some(Receive::ByMutRef))
    }

    pub fn borrow_method(&self) -> Identifier {
        match &self.kind {
            VectorKind::Primitive(kind) => Identifier::known(kind.borrow_method()),
            VectorKind::Record(_) => Identifier::known("borrowRecordArray"),
        }
    }

    pub fn borrow(&self, pointer: Expression, length: Expression) -> Expression {
        match &self.kind {
            VectorKind::Primitive(kind) => Expression::call(
                Expression::identifier(Identifier::known("_module")),
                Identifier::known(kind.borrow_method()),
                [pointer, length].into_iter().collect(),
            ),
            VectorKind::Record(record) => record.borrow(pointer, length),
        }
    }

    pub const fn alignment(&self) -> usize {
        match &self.kind {
            VectorKind::Primitive(kind) => kind.alignment(),
            VectorKind::Record(record) => record.alignment,
        }
    }

    pub fn return_slot_method(&self) -> Identifier {
        match self.kind {
            VectorKind::Primitive(_) => Identifier::known("writeReturnSlot"),
            VectorKind::Record(_) => Identifier::known("writeWriterReturnSlot"),
        }
    }

    pub fn free_method(&self) -> Identifier {
        match self.kind {
            VectorKind::Primitive(_) => Identifier::known("freePrimitiveBuffer"),
            VectorKind::Record(_) => Identifier::known("freeWriter"),
        }
    }

    pub fn element_literal(&self) -> StringLiteral {
        match self.kind {
            VectorKind::Primitive(kind) => StringLiteral::new(kind.element_name()),
            VectorKind::Record(_) => StringLiteral::new("record"),
        }
    }

    fn unsupported<T>(shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }
}

impl RecordVector {
    fn new(id: boltffi_binding::RecordId, context: &RenderContext<Wasm32>) -> Result<Self> {
        let Some(record) = context.record(id) else {
            return DirectVector::unsupported("direct vector record without declaration");
        };
        let RecordDecl::Direct(record) = record else {
            return DirectVector::unsupported("encoded direct vector record");
        };
        Ok(Self {
            ty: Name::new(record.name()).type_name(),
            codec: Name::new(record.name()).codec_identifier()?,
            size: record.layout().size().get(),
            alignment: record.layout().alignment().get() as usize,
        })
    }

    fn allocation(&self, value: Expression) -> Expression {
        let writer = Identifier::known("writer");
        let element = Identifier::known("element");
        Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("allocCompositeBuffer"),
            [
                value,
                Expression::integer(self.size),
                Expression::parameters_lambda(
                    [writer.clone(), element.clone()],
                    Expression::call(
                        Expression::identifier(self.codec.clone()),
                        Identifier::known("encode"),
                        [
                            Expression::identifier(writer),
                            Expression::identifier(element),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn take(&self) -> Expression {
        let reader = Identifier::known("reader");
        Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("takeSlotRecordArray"),
            [
                Expression::integer(self.size),
                Expression::parameter_lambda(
                    reader.clone(),
                    Expression::call(
                        Expression::identifier(self.codec.clone()),
                        Identifier::known("decode"),
                        [Expression::identifier(reader)].into_iter().collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }

    fn borrow(&self, pointer: Expression, length: Expression) -> Expression {
        let reader = Identifier::known("reader");
        Expression::call(
            Expression::identifier(Identifier::known("_module")),
            Identifier::known("borrowRecordArray"),
            [
                pointer,
                length,
                Expression::integer(self.size),
                Expression::parameter_lambda(
                    reader.clone(),
                    Expression::call(
                        Expression::identifier(self.codec.clone()),
                        Identifier::known("decode"),
                        [Expression::identifier(reader)].into_iter().collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}

impl PrimitiveVector {
    fn new(primitive: Primitive) -> Result<Self> {
        Ok(match primitive {
            Primitive::Bool => Self::Bool,
            Primitive::I8 => Self::I8,
            Primitive::I16 => Self::I16,
            Primitive::U16 => Self::U16,
            Primitive::I32 | Primitive::ISize => Self::I32,
            Primitive::U32 | Primitive::USize => Self::U32,
            Primitive::I64 => Self::I64,
            Primitive::U64 => Self::U64,
            Primitive::F32 => Self::F32,
            Primitive::F64 => Self::F64,
            Primitive::U8 => return DirectVector::unsupported("u8 direct vector"),
            _ => return DirectVector::unsupported("unknown direct vector primitive"),
        })
    }

    fn primitive(self) -> Primitive {
        match self {
            Self::Bool => Primitive::Bool,
            Self::I8 => Primitive::I8,
            Self::I16 => Primitive::I16,
            Self::U16 => Primitive::U16,
            Self::I32 => Primitive::I32,
            Self::U32 => Primitive::U32,
            Self::I64 => Primitive::I64,
            Self::U64 => Primitive::U64,
            Self::F32 => Primitive::F32,
            Self::F64 => Primitive::F64,
        }
    }

    fn typed_array(self) -> TypeName {
        TypeName::named(match self {
            Self::Bool => "Uint8Array",
            Self::I8 => "Int8Array",
            Self::I16 => "Int16Array",
            Self::U16 => "Uint16Array",
            Self::I32 => "Int32Array",
            Self::U32 => "Uint32Array",
            Self::I64 => "BigInt64Array",
            Self::U64 => "BigUint64Array",
            Self::F32 => "Float32Array",
            Self::F64 => "Float64Array",
        })
    }

    fn allocation_method(self) -> &'static str {
        match self {
            Self::Bool => "allocBoolArray",
            Self::I8 => "allocI8Array",
            Self::I16 => "allocI16Array",
            Self::U16 => "allocU16Array",
            Self::I32 => "allocI32Array",
            Self::U32 => "allocU32Array",
            Self::I64 => "allocI64Array",
            Self::U64 => "allocU64Array",
            Self::F32 => "allocF32Array",
            Self::F64 => "allocF64Array",
        }
    }

    fn take_method(self) -> &'static str {
        match self {
            Self::Bool => "takeSlotBoolArray",
            Self::I8 => "takeSlotI8Array",
            Self::I16 => "takeSlotI16Array",
            Self::U16 => "takeSlotU16Array",
            Self::I32 => "takeSlotI32Array",
            Self::U32 => "takeSlotU32Array",
            Self::I64 => "takeSlotI64Array",
            Self::U64 => "takeSlotU64Array",
            Self::F32 => "takeSlotF32Array",
            Self::F64 => "takeSlotF64Array",
        }
    }

    fn borrow_method(self) -> &'static str {
        match self {
            Self::Bool => "borrowBoolArray",
            Self::I8 => "borrowI8Array",
            Self::I16 => "borrowI16Array",
            Self::U16 => "borrowU16Array",
            Self::I32 => "borrowI32Array",
            Self::U32 => "borrowU32Array",
            Self::I64 => "borrowI64Array",
            Self::U64 => "borrowU64Array",
            Self::F32 => "borrowF32Array",
            Self::F64 => "borrowF64Array",
        }
    }

    const fn alignment(self) -> usize {
        match self {
            Self::Bool | Self::I8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    fn element_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}
