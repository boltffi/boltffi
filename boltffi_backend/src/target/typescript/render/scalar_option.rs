use boltffi_binding::Primitive;

use crate::core::{Error, Result};

use super::super::syntax::{Expression, Identifier, TypeName};
use super::Type;

pub struct ScalarOption {
    kind: ScalarKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ScalarKind {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F64,
}

impl ScalarOption {
    pub fn new(primitive: Primitive) -> Result<Self> {
        let kind = match primitive {
            Primitive::Bool => ScalarKind::Bool,
            Primitive::I8 => ScalarKind::I8,
            Primitive::U8 => ScalarKind::U8,
            Primitive::I16 => ScalarKind::I16,
            Primitive::U16 => ScalarKind::U16,
            Primitive::I32 | Primitive::ISize => ScalarKind::I32,
            Primitive::U32 | Primitive::USize => ScalarKind::U32,
            Primitive::F64 => ScalarKind::F64,
            Primitive::I64 | Primitive::U64 | Primitive::F32 => {
                return Self::unsupported("lossy Wasm scalar option");
            }
            _ => return Self::unsupported("unknown Wasm scalar option"),
        };
        Ok(Self { kind })
    }

    pub fn ty(&self) -> Result<TypeName> {
        Type::primitive(self.kind.primitive()).map(TypeName::nullable)
    }

    pub fn argument(&self, value: Expression) -> Expression {
        if matches!(self.kind, ScalarKind::F64) {
            return Expression::call(
                Expression::identifier(Identifier::known("_module")),
                self.pack_method(),
                [value].into_iter().collect(),
            );
        }
        let present = match self.kind {
            ScalarKind::Bool => value
                .clone()
                .conditional(Expression::integer(1), Expression::integer(0)),
            _ => value.clone(),
        };
        value
            .strict_equal(Expression::null())
            .conditional(Expression::nan(), present)
    }

    pub fn unpack_method(&self) -> Identifier {
        Identifier::known(match self.kind {
            ScalarKind::Bool => "unpackOptionBool",
            ScalarKind::I8 => "unpackOptionI8",
            ScalarKind::U8 => "unpackOptionU8",
            ScalarKind::I16 => "unpackOptionI16",
            ScalarKind::U16 => "unpackOptionU16",
            ScalarKind::I32 => "unpackOptionI32",
            ScalarKind::U32 => "unpackOptionU32",
            ScalarKind::F64 => "unpackOptionF64Bits",
        })
    }

    pub fn return_unpack_method(&self) -> Identifier {
        Identifier::known(match self.kind {
            ScalarKind::Bool => "unpackOptionBool",
            ScalarKind::I8 => "unpackOptionI8",
            ScalarKind::U8 => "unpackOptionU8",
            ScalarKind::I16 => "unpackOptionI16",
            ScalarKind::U16 => "unpackOptionU16",
            ScalarKind::I32 => "unpackOptionI32",
            ScalarKind::U32 => "unpackOptionU32",
            ScalarKind::F64 => "unpackOptionF64",
        })
    }

    pub fn pack_method(&self) -> Identifier {
        Identifier::known(match self.kind {
            ScalarKind::F64 => "packOptionF64Bits",
            _ => "packOptionScalar",
        })
    }

    pub fn carrier_type(&self) -> TypeName {
        match self.kind {
            ScalarKind::F64 => TypeName::bigint(),
            _ => TypeName::number(),
        }
    }

    fn unsupported<T>(shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }
}

impl ScalarKind {
    fn primitive(self) -> Primitive {
        match self {
            Self::Bool => Primitive::Bool,
            Self::I8 => Primitive::I8,
            Self::U8 => Primitive::U8,
            Self::I16 => Primitive::I16,
            Self::U16 => Primitive::U16,
            Self::I32 => Primitive::I32,
            Self::U32 => Primitive::U32,
            Self::F64 => Primitive::F64,
        }
    }
}
