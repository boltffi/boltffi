use boltffi_binding::{
    DirectValueType, FieldKey, IntrinsicOp, OpRender, Primitive, ValueRef, Wasm32,
};

use crate::core::{Error, Result};

use super::super::syntax::{ArgumentList, Expression, Identifier, IntegerLiteral, PropertyKey};
use super::value::ValueExpression;

pub struct Operation {
    value: Expression,
}

impl Operation {
    pub fn new(current: Expression, value: &ValueRef) -> Result<Self> {
        Ok(Self {
            value: ValueExpression::new(value, current).render()?,
        })
    }

    fn single_argument(arguments: Vec<Result<Expression>>) -> Result<Expression> {
        let mut arguments = arguments.into_iter();
        let argument = arguments
            .next()
            .ok_or_else(|| Self::error("missing operation argument"))??;
        match arguments.next() {
            Some(_) => Err(Self::error("extra operation argument")),
            None => Ok(argument),
        }
    }

    fn primitive_size(primitive: Primitive) -> Expression {
        Expression::integer(primitive.byte_size::<Wasm32>().get())
    }

    fn error(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: "typescript",
            shape,
        }
    }
}

impl OpRender for Operation {
    type Expr = Result<Expression>;

    fn value(&mut self, _: &ValueRef) -> Self::Expr {
        Ok(self.value.clone())
    }

    fn byte_count(&mut self, bytes: u64) -> Self::Expr {
        Ok(Expression::integer(bytes))
    }

    fn integer(&mut self, value: i128) -> Self::Expr {
        Ok(Expression::integer_literal(IntegerLiteral::number(value)))
    }

    fn add(&mut self, left: Self::Expr, right: Self::Expr) -> Self::Expr {
        Ok(left?.add(right?))
    }

    fn mul(&mut self, left: Self::Expr, right: Self::Expr) -> Self::Expr {
        Ok(left?.multiply(right?))
    }

    fn eq(&mut self, left: Self::Expr, right: Self::Expr) -> Self::Expr {
        Ok(left?.strict_equal(right?))
    }

    fn field(&mut self, base: Self::Expr, field: &FieldKey) -> Self::Expr {
        PropertyKey::from_field(field)?.access(base?)
    }

    fn intrinsic(&mut self, intrinsic: IntrinsicOp, arguments: Vec<Self::Expr>) -> Self::Expr {
        let value = Self::single_argument(arguments)?;
        match intrinsic {
            IntrinsicOp::Utf8ByteCount => Ok(Expression::invoke(
                Identifier::known("utf8ByteCount"),
                [value].into_iter().collect::<ArgumentList>(),
            )),
            IntrinsicOp::SequenceLen => {
                Ok(Expression::property(value, Identifier::known("length")))
            }
            IntrinsicOp::WireSize => Ok(Expression::call(
                value,
                Identifier::known("wireSize"),
                ArgumentList::default(),
            )),
            _ => Err(Self::error("unknown intrinsic operation")),
        }
    }

    fn size_of(&mut self, ty: &DirectValueType) -> Self::Expr {
        match ty {
            DirectValueType::Primitive(primitive) => Ok(Self::primitive_size(*primitive)),
            _ => Err(Self::error("aggregate operation size")),
        }
    }
}
