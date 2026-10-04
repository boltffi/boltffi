use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, MapKind, Op,
    Primitive, RecordId,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::fixed::{self, FixedStruct},
        cpython::render::primitive,
        render::Package,
        syntax::{CallExpression, Expression, Identifier},
    },
};

pub struct Reader<'package> {
    package: &'package Package<'package>,
}

/// A rendered read expression paired with its sequence fast lane.
///
/// A sequence decodes element-by-element through a closure unless every
/// element occupies a fixed number of wire bytes; the lane names the bulk
/// decode the runtime can use instead.
pub struct ReadValue {
    expression: Expression,
    lane: SequenceLane,
}

enum SequenceLane {
    General,
    FixedRecord {
        class: Identifier,
        spelling: FixedStruct,
    },
    CStyleEnum {
        class: Identifier,
        primitive: Primitive,
    },
}

impl ReadValue {
    fn general(expression: Expression) -> Self {
        Self {
            expression,
            lane: SequenceLane::General,
        }
    }

    pub fn into_expression(self) -> Expression {
        self.expression
    }
}

impl<'package> Reader<'package> {
    pub fn new(package: &'package Package<'package>) -> Self {
        Self { package }
    }

    pub fn sequence_expression(&self, element: ReadValue) -> Result<Expression> {
        match element.lane {
            SequenceLane::FixedRecord { class, spelling } => Ok(Expression::call(
                CallExpression::new(Self::reader_attribute(Identifier::parse(
                    "fixed_sequence",
                )?)?)
                .positional(Expression::identifier(spelling.struct_global().clone()))
                .positional(Expression::identifier(class)),
            )),
            SequenceLane::CStyleEnum { class, primitive } => Ok(Expression::call(
                CallExpression::new(Self::reader_attribute(Identifier::parse("enum_sequence")?)?)
                    .positional(Expression::identifier(fixed::scalar_struct_global(
                        primitive,
                    )?))
                    .positional(Expression::identifier(class)),
            )),
            SequenceLane::General => Ok(Expression::call(
                CallExpression::new(Self::reader_attribute(Identifier::parse("sequence")?)?)
                    .positional(Expression::no_arg_lambda(element.expression)),
            )),
        }
    }

    fn reader_attribute(method: Identifier) -> Result<Expression> {
        Ok(Expression::attribute(
            Expression::identifier(Identifier::parse("reader")?),
            method,
        ))
    }

    fn reader_call(method: Identifier) -> Result<Expression> {
        Ok(Expression::call(CallExpression::new(
            Self::reader_attribute(method)?,
        )))
    }

    fn from_reader_expression(class: &Identifier) -> Result<Expression> {
        Ok(Expression::call(
            CallExpression::new(Expression::attribute(
                Expression::identifier(class.clone()),
                Identifier::parse("_boltffi_from_reader")?,
            ))
            .positional(Expression::identifier(Identifier::parse("reader")?)),
        ))
    }
}

impl<'package> CodecRead for Reader<'package> {
    type Expr = Result<ReadValue>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        let stem = primitive::Runtime::new(primitive).wire_stem()?;
        Self::reader_call(Identifier::parse(stem)?).map(ReadValue::general)
    }

    fn string(&mut self) -> Self::Expr {
        Self::reader_call(Identifier::parse("string")?).map(ReadValue::general)
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        unreachable!(
            "InternedString codec read reached Python renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Expr {
        Self::reader_call(Identifier::parse("bytes")?).map(ReadValue::general)
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        let class = self.package.record_name(id)?;
        Ok(ReadValue {
            expression: Self::from_reader_expression(&class)?,
            lane: SequenceLane::FixedRecord {
                spelling: self.package.direct_record_struct(id)?,
                class,
            },
        })
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        Self::from_reader_expression(&self.package.record_name(id)?).map(ReadValue::general)
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        let EnumCodec::CStyle(primitive) = self.package.enum_codec(id)? else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum reached c-style wire reader",
            });
        };
        let class = self.package.enum_name(id)?;
        let stem = primitive::Runtime::new(primitive).wire_stem()?;
        let expression = Expression::call(
            CallExpression::new(Expression::identifier(class.clone()))
                .positional(Self::reader_call(Identifier::parse(stem)?)?),
        );
        Ok(ReadValue {
            expression,
            lane: SequenceLane::CStyleEnum { class, primitive },
        })
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        let EnumCodec::Data { class_name, .. } = self.package.enum_codec(id)? else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "c-style enum reached data enum wire reader",
            });
        };
        Ok(ReadValue::general(Expression::call(
            CallExpression::new(Expression::attribute(
                Expression::identifier(class_name),
                Identifier::parse("_boltffi_from_reader")?,
            ))
            .positional(Expression::identifier(Identifier::parse("reader")?)),
        )))
    }

    fn class_handle(&mut self, id: ClassId) -> Self::Expr {
        self.package.class_name(&id)?;
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "class handle in wire reader",
        })
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "callback handle in wire reader",
        })
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        self.package.custom_type(id)?;
        representation.map(|value| ReadValue::general(value.expression))
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        Ok(ReadValue::general(match kind {
            BuiltinType::Duration => Self::reader_call(Identifier::parse("duration")?)?,
            BuiltinType::SystemTime => Self::reader_call(Identifier::parse("system_time")?)?,
            BuiltinType::Uuid => Self::reader_call(Identifier::parse("uuid")?)?,
            BuiltinType::Url => Self::reader_call(Identifier::parse("url")?)?,
        }))
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(ReadValue::general(Expression::call(
            CallExpression::new(Self::reader_attribute(Identifier::parse("optional")?)?)
                .positional(Expression::no_arg_lambda(inner?.expression)),
        )))
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        self.sequence_expression(element?).map(ReadValue::general)
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        elements
            .into_iter()
            .map(|element| element.map(ReadValue::into_expression))
            .collect::<Result<Vec<_>>>()
            .map(Expression::tuple)
            .map(ReadValue::general)
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(ReadValue::general(Expression::call(
            CallExpression::new(Self::reader_attribute(Identifier::parse("result")?)?)
                .positional(Expression::no_arg_lambda(ok?.expression))
                .positional(Expression::no_arg_lambda(err?.expression)),
        )))
    }

    fn map(&mut self, kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        let method = match kind {
            MapKind::Hash | MapKind::BTree => Identifier::parse("map")?,
        };
        Ok(ReadValue::general(Expression::call(
            CallExpression::new(Self::reader_attribute(method)?)
                .positional(Expression::no_arg_lambda(key?.expression))
                .positional(Expression::no_arg_lambda(value?.expression)),
        )))
    }
}

pub enum EnumCodec {
    CStyle(Primitive),
    Data {
        class_name: Identifier,
        transparent: bool,
    },
}
