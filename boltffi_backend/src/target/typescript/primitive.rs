use boltffi_binding::Primitive;

use crate::core::{Error, Result};

use super::syntax::{Identifier, TypeName};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Scalar {
    primitive: Primitive,
}

impl Scalar {
    pub fn new(primitive: Primitive) -> Result<Self> {
        match primitive {
            Primitive::Bool
            | Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32
            | Primitive::F64 => Ok(Self { primitive }),
            _ => Err(Error::UnsupportedTarget {
                target: "typescript",
                shape: "unknown primitive",
            }),
        }
    }

    pub fn ty(self) -> TypeName {
        match self.primitive {
            Primitive::Bool => TypeName::boolean(),
            Primitive::I64 | Primitive::U64 => TypeName::bigint(),
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32
            | Primitive::F64 => TypeName::number(),
            _ => unreachable!(),
        }
    }

    pub fn read_method(self) -> Identifier {
        Identifier::known(match self.primitive {
            Primitive::Bool => "readBool",
            Primitive::I8 => "readI8",
            Primitive::U8 => "readU8",
            Primitive::I16 => "readI16",
            Primitive::U16 => "readU16",
            Primitive::I32 => "readI32",
            Primitive::U32 => "readU32",
            Primitive::I64 => "readI64",
            Primitive::U64 => "readU64",
            Primitive::ISize => "readISize",
            Primitive::USize => "readUSize",
            Primitive::F32 => "readF32",
            Primitive::F64 => "readF64",
            _ => unreachable!(),
        })
    }

    pub fn write_method(self) -> Identifier {
        Identifier::known(match self.primitive {
            Primitive::Bool => "writeBool",
            Primitive::I8 => "writeI8",
            Primitive::U8 => "writeU8",
            Primitive::I16 => "writeI16",
            Primitive::U16 => "writeU16",
            Primitive::I32 => "writeI32",
            Primitive::U32 => "writeU32",
            Primitive::I64 => "writeI64",
            Primitive::U64 => "writeU64",
            Primitive::ISize => "writeISize",
            Primitive::USize => "writeUSize",
            Primitive::F32 => "writeF32",
            Primitive::F64 => "writeF64",
            _ => unreachable!(),
        })
    }

    pub fn read_array_method(self) -> Identifier {
        Identifier::known(match self.primitive {
            Primitive::Bool => "readBoolArray",
            Primitive::I8 => "readI8Array",
            Primitive::U8 => "readU8Array",
            Primitive::I16 => "readI16Array",
            Primitive::U16 => "readU16Array",
            Primitive::I32 => "readI32Array",
            Primitive::U32 => "readU32Array",
            Primitive::I64 => "readI64Array",
            Primitive::U64 => "readU64Array",
            Primitive::ISize => "readISizeArray",
            Primitive::USize => "readUSizeArray",
            Primitive::F32 => "readF32Array",
            Primitive::F64 => "readF64Array",
            _ => unreachable!(),
        })
    }

    pub fn take_optional_method(self) -> Identifier {
        Identifier::known(match self.primitive {
            Primitive::Bool => "takePackedOptionalBool",
            Primitive::I8 => "takePackedOptionalI8",
            Primitive::U8 => "takePackedOptionalU8",
            Primitive::I16 => "takePackedOptionalI16",
            Primitive::U16 => "takePackedOptionalU16",
            Primitive::I32 | Primitive::ISize => "takePackedOptionalI32",
            Primitive::U32 | Primitive::USize => "takePackedOptionalU32",
            Primitive::I64 => "takePackedOptionalI64",
            Primitive::U64 => "takePackedOptionalU64",
            Primitive::F32 => "takePackedOptionalF32",
            Primitive::F64 => "takePackedOptionalF64",
            _ => unreachable!(),
        })
    }

    pub fn typed_array(self) -> Option<TypeName> {
        Some(TypeName::named(match self.primitive {
            Primitive::Bool => return None,
            Primitive::I8 => "Int8Array",
            Primitive::U8 => "Uint8Array",
            Primitive::I16 => "Int16Array",
            Primitive::U16 => "Uint16Array",
            Primitive::I32 | Primitive::ISize => "Int32Array",
            Primitive::U32 | Primitive::USize => "Uint32Array",
            Primitive::I64 => "BigInt64Array",
            Primitive::U64 => "BigUint64Array",
            Primitive::F32 => "Float32Array",
            Primitive::F64 => "Float64Array",
            _ => unreachable!(),
        }))
    }
}
