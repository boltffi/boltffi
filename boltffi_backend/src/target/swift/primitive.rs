use boltffi_binding::Primitive;

use crate::{
    core::Result,
    target::swift::{
        SwiftHost,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SwiftPrimitive {
    primitive: Primitive,
}

impl SwiftPrimitive {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }

    pub fn api_type(self) -> Result<TypeName> {
        match self.primitive {
            Primitive::Bool => Ok(TypeName::bool()),
            Primitive::I8 => Ok(TypeName::int8()),
            Primitive::U8 => Ok(TypeName::uint8()),
            Primitive::I16 => Ok(TypeName::int16()),
            Primitive::U16 => Ok(TypeName::uint16()),
            Primitive::I32 => Ok(TypeName::int32()),
            Primitive::U32 => Ok(TypeName::uint32()),
            Primitive::I64 => Ok(TypeName::int64()),
            Primitive::U64 => Ok(TypeName::uint64()),
            Primitive::ISize => Ok(TypeName::int()),
            Primitive::USize => Ok(TypeName::uint()),
            Primitive::F32 => Ok(TypeName::float()),
            Primitive::F64 => Ok(TypeName::double()),
            _ => Err(SwiftHost::unsupported("unknown primitive")),
        }
    }

    pub fn read_method(self) -> Result<Identifier> {
        self.method("read")
    }

    pub fn write_method(self) -> Result<Identifier> {
        self.method("write")
    }

    pub fn read_expression(self, reader: Identifier) -> Result<Expression> {
        let value = Expression::call(
            Expression::member(reader, self.read_method()?),
            ArgumentList::default(),
        );
        Ok(match self.primitive {
            Primitive::ISize => Expression::call("Int", [value].into_iter().collect()),
            Primitive::USize => Expression::call("UInt", [value].into_iter().collect()),
            _ => value,
        })
    }

    pub fn write_statement(self, writer: Identifier, value: Expression) -> Result<Statement> {
        let value = match self.primitive {
            Primitive::ISize => Expression::call("Int64", [value].into_iter().collect()),
            Primitive::USize => Expression::call("UInt64", [value].into_iter().collect()),
            _ => value,
        };
        Ok(Statement::expression(Expression::call(
            Expression::member(writer, self.write_method()?),
            [value].into_iter().collect(),
        )))
    }

    fn method(self, prefix: &'static str) -> Result<Identifier> {
        self.method_suffix()
            .and_then(|suffix| Identifier::parse(format!("{prefix}{suffix}")))
    }

    fn method_suffix(self) -> Result<&'static str> {
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
            _ => Err(SwiftHost::unsupported("unknown primitive wire method")),
        }
    }
}
