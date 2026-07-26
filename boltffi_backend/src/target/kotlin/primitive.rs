use boltffi_binding::{IntegerValue, Primitive};

use crate::{
    core::Result,
    target::kotlin::{
        KotlinHost,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

pub struct KotlinPrimitive {
    primitive: Primitive,
}

impl KotlinPrimitive {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }

    pub fn api_type(self) -> Result<TypeName> {
        Ok(match self.primitive {
            Primitive::Bool => TypeName::boolean(),
            Primitive::I8 => TypeName::byte(),
            Primitive::U8 => TypeName::ubyte(),
            Primitive::I16 => TypeName::short(),
            Primitive::U16 => TypeName::ushort(),
            Primitive::I32 => TypeName::int(),
            Primitive::U32 => TypeName::uint(),
            Primitive::I64 | Primitive::ISize => TypeName::long(),
            Primitive::U64 | Primitive::USize => TypeName::ulong(),
            Primitive::F32 => TypeName::float(),
            Primitive::F64 => TypeName::double(),
            _ => {
                return Err(KotlinHost::unsupported("unknown primitive type"));
            }
        })
    }

    pub fn native_type(self) -> Result<TypeName> {
        Ok(match self.primitive {
            Primitive::Bool => TypeName::boolean(),
            Primitive::I8 | Primitive::U8 => TypeName::byte(),
            Primitive::I16 | Primitive::U16 => TypeName::short(),
            Primitive::I32 | Primitive::U32 => TypeName::int(),
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize => {
                TypeName::long()
            }
            Primitive::F32 => TypeName::float(),
            Primitive::F64 => TypeName::double(),
            _ => {
                return Err(KotlinHost::unsupported("unknown native primitive type"));
            }
        })
    }

    pub fn direct_vector_type(self) -> Result<TypeName> {
        Ok(match self.primitive {
            Primitive::Bool => TypeName::new("BooleanArray"),
            Primitive::I8 => TypeName::new("ByteArray"),
            Primitive::U8 => TypeName::new("ByteArray"),
            Primitive::I16 => TypeName::new("ShortArray"),
            Primitive::U16 => TypeName::new("UShortArray"),
            Primitive::I32 => TypeName::new("IntArray"),
            Primitive::U32 => TypeName::new("UIntArray"),
            Primitive::I64 | Primitive::ISize => TypeName::new("LongArray"),
            Primitive::U64 | Primitive::USize => TypeName::new("ULongArray"),
            Primitive::F32 => TypeName::new("FloatArray"),
            Primitive::F64 => TypeName::new("DoubleArray"),
            _ => {
                return Err(KotlinHost::unsupported("unknown direct-vector primitive"));
            }
        })
    }

    pub fn native_argument(self, value: Expression) -> Result<Expression> {
        Ok(
            match self.conversion("toByte", "toShort", "toInt", "toLong")? {
                Some(method) => value.convert(method),
                None => value,
            },
        )
    }

    pub fn public_return(self, value: Expression) -> Result<Expression> {
        Ok(
            match self.conversion("toUByte", "toUShort", "toUInt", "toULong")? {
                Some(method) => value.convert(method),
                None => value,
            },
        )
    }

    pub fn wire_size(self) -> Result<u64> {
        match self.primitive {
            Primitive::Bool | Primitive::I8 | Primitive::U8 => Ok(1),
            Primitive::I16 | Primitive::U16 => Ok(2),
            Primitive::I32 | Primitive::U32 | Primitive::F32 => Ok(4),
            Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F64 => Ok(8),
            _ => Err(KotlinHost::unsupported("unknown primitive wire size")),
        }
    }

    pub fn wire_method_suffix(self) -> Result<&'static str> {
        match self.primitive {
            Primitive::Bool => Ok("Bool"),
            Primitive::I8 => Ok("I8"),
            Primitive::U8 => Ok("U8"),
            Primitive::I16 => Ok("I16"),
            Primitive::U16 => Ok("U16"),
            Primitive::I32 => Ok("I32"),
            Primitive::U32 => Ok("U32"),
            Primitive::I64 | Primitive::ISize => Ok("I64"),
            Primitive::U64 | Primitive::USize => Ok("U64"),
            Primitive::F32 => Ok("F32"),
            Primitive::F64 => Ok("F64"),
            _ => Err(KotlinHost::unsupported("unknown primitive wire method")),
        }
    }

    pub fn native_wire_method_suffix(self) -> Result<&'static str> {
        match self.primitive {
            Primitive::Bool => Ok("Bool"),
            Primitive::I8 | Primitive::U8 => Ok("I8"),
            Primitive::I16 | Primitive::U16 => Ok("I16"),
            Primitive::I32 | Primitive::U32 => Ok("I32"),
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize => Ok("I64"),
            Primitive::F32 => Ok("F32"),
            Primitive::F64 => Ok("F64"),
            _ => Err(KotlinHost::unsupported(
                "unknown primitive native wire method",
            )),
        }
    }

    pub fn wire_array_method_suffix(self) -> Result<&'static str> {
        match self.primitive {
            Primitive::Bool => Ok("Boolean"),
            Primitive::I8 | Primitive::U8 => Ok("Byte"),
            Primitive::I16 => Ok("Short"),
            Primitive::U16 => Ok("UShort"),
            Primitive::I32 => Ok("Int"),
            Primitive::U32 => Ok("UInt"),
            Primitive::I64 | Primitive::ISize => Ok("Long"),
            Primitive::U64 | Primitive::USize => Ok("ULong"),
            Primitive::F32 => Ok("Float"),
            Primitive::F64 => Ok("Double"),
            _ => Err(KotlinHost::unsupported("unknown primitive array method")),
        }
    }

    pub fn integer_literal(self, value: IntegerValue) -> Result<Expression> {
        let signed = value.get();
        let value = Expression::integer(signed);
        let converted = match signed < 0 {
            true => value.clone().parenthesized(),
            false => value.clone(),
        };
        Ok(match self.primitive {
            Primitive::I8 => converted.convert(Identifier::parse("toByte")?),
            Primitive::U8 => converted.convert(Identifier::parse("toUByte")?),
            Primitive::I16 => converted.convert(Identifier::parse("toShort")?),
            Primitive::U16 => converted.convert(Identifier::parse("toUShort")?),
            Primitive::U32 => converted.convert(Identifier::parse("toUInt")?),
            Primitive::U64 | Primitive::USize => converted.convert(Identifier::parse("toULong")?),
            Primitive::I32 | Primitive::I64 | Primitive::ISize => value,
            _ => {
                return Err(KotlinHost::unsupported("unknown primitive literal"));
            }
        })
    }

    pub fn native_integer_literal(self, value: IntegerValue) -> Result<Expression> {
        let signed = value.get();
        let expression = Expression::integer(signed);
        let converted = match signed < 0 {
            true => expression.clone().parenthesized(),
            false => expression.clone(),
        };
        Ok(match self.primitive {
            Primitive::I8 | Primitive::U8 => converted.convert(Identifier::parse("toByte")?),
            Primitive::I16 | Primitive::U16 => converted.convert(Identifier::parse("toShort")?),
            Primitive::I32 => expression,
            Primitive::U32 if i32::try_from(signed).is_ok() => expression,
            Primitive::U32 => Expression::long(signed).convert(Identifier::parse("toInt")?),
            Primitive::I64 | Primitive::ISize => Expression::long(signed),
            Primitive::U64 | Primitive::USize if i64::try_from(signed).is_ok() => {
                Expression::long(signed)
            }
            Primitive::U64 | Primitive::USize => {
                Expression::unsigned_long(signed as u128).convert(Identifier::parse("toLong")?)
            }
            _ => {
                return Err(KotlinHost::unsupported("unknown native primitive literal"));
            }
        })
    }

    pub fn buffer_read_at(self, buffer: &Identifier, offset: Expression) -> Result<Expression> {
        match self.primitive {
            Primitive::Bool => Ok(Self::buffer_call(buffer, "get", [offset])?
                .not_equal(Expression::integer(0).convert(Identifier::parse("toByte")?))),
            Primitive::I8 => Self::buffer_call(buffer, "get", [offset]),
            Primitive::U8 => Self::buffer_call(buffer, "get", [offset])
                .and_then(|value| Self::converted(value, "toUByte")),
            Primitive::I16 => Self::buffer_call(buffer, "getShort", [offset]),
            Primitive::U16 => Self::buffer_call(buffer, "getShort", [offset])
                .and_then(|value| Self::converted(value, "toUShort")),
            Primitive::I32 => Self::buffer_call(buffer, "getInt", [offset]),
            Primitive::U32 => Self::buffer_call(buffer, "getInt", [offset])
                .and_then(|value| Self::converted(value, "toUInt")),
            Primitive::I64 => Self::buffer_call(buffer, "getLong", [offset]),
            Primitive::U64 => Self::buffer_call(buffer, "getLong", [offset])
                .and_then(|value| Self::converted(value, "toULong")),
            Primitive::F32 => Self::buffer_call(buffer, "getFloat", [offset]),
            Primitive::F64 => Self::buffer_call(buffer, "getDouble", [offset]),
            _ => Err(KotlinHost::unsupported("unknown direct record field read")),
        }
    }

    pub fn buffer_write_at(
        self,
        buffer: &Identifier,
        offset: Expression,
        value: Expression,
    ) -> Result<Statement> {
        let (method, value) = match self.primitive {
            Primitive::Bool => (
                "put",
                Expression::conditional(
                    value,
                    Expression::integer(1).convert(Identifier::parse("toByte")?),
                    Expression::integer(0).convert(Identifier::parse("toByte")?),
                ),
            ),
            Primitive::I8 => ("put", value),
            Primitive::U8 => ("put", value.convert(Identifier::parse("toByte")?)),
            Primitive::I16 => ("putShort", value),
            Primitive::U16 => ("putShort", value.convert(Identifier::parse("toShort")?)),
            Primitive::I32 => ("putInt", value),
            Primitive::U32 => ("putInt", value.convert(Identifier::parse("toInt")?)),
            Primitive::I64 => ("putLong", value),
            Primitive::U64 => ("putLong", value.convert(Identifier::parse("toLong")?)),
            Primitive::F32 => ("putFloat", value),
            Primitive::F64 => ("putDouble", value),
            _ => {
                return Err(KotlinHost::unsupported("unknown direct record field write"));
            }
        };
        Self::buffer_call(buffer, method, [offset, value]).map(Statement::expression)
    }

    fn conversion(
        self,
        u8_method: &'static str,
        u16_method: &'static str,
        u32_method: &'static str,
        u64_method: &'static str,
    ) -> Result<Option<Identifier>> {
        match self.primitive {
            Primitive::U8 => Some(Identifier::parse(u8_method)),
            Primitive::U16 => Some(Identifier::parse(u16_method)),
            Primitive::U32 => Some(Identifier::parse(u32_method)),
            Primitive::U64 | Primitive::USize => Some(Identifier::parse(u64_method)),
            Primitive::Bool
            | Primitive::I8
            | Primitive::I16
            | Primitive::I32
            | Primitive::I64
            | Primitive::ISize
            | Primitive::F32
            | Primitive::F64 => None,
            _ => {
                return Err(KotlinHost::unsupported("unknown primitive conversion"));
            }
        }
        .transpose()
    }

    fn buffer_call(
        buffer: &Identifier,
        method: &'static str,
        arguments: impl IntoIterator<Item = Expression>,
    ) -> Result<Expression> {
        Ok(Expression::call(
            Expression::identifier(buffer.clone()),
            Identifier::escape(method)?,
            arguments.into_iter().collect::<ArgumentList>(),
        ))
    }

    fn converted(value: Expression, method: &'static str) -> Result<Expression> {
        Identifier::parse(method).map(|method| value.convert(method))
    }
}
