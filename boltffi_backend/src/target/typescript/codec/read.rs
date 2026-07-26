use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, MapKind, Op,
    Primitive, RecordId, Wasm32,
};

use crate::core::{Error, RenderContext, Result};

use super::super::{
    name_style::Name,
    primitive::Scalar,
    syntax::{ArgumentList, Expression, Identifier},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReadKind {
    Primitive(Primitive),
    CustomPrimitive(Primitive),
    OptionalPrimitive(Primitive),
    String,
    Utf8String,
    Bytes,
    ErrorRecord(RecordId),
    ErrorEnum(EnumId),
}

pub struct ReadExpression {
    expression: Expression,
    kind: Option<ReadKind>,
}

pub struct Reader<'context> {
    reader: Identifier,
    context: &'context RenderContext<'context, Wasm32>,
}

impl<'context> Reader<'context> {
    pub fn new(reader: Identifier, context: &'context RenderContext<'context, Wasm32>) -> Self {
        Self { reader, context }
    }

    fn unsupported(shape: &'static str) -> Result<ReadExpression> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }

    fn record(&self, id: RecordId) -> Result<ReadExpression> {
        let record = self.context.record(id).ok_or(Error::UnsupportedTarget {
            target: "typescript",
            shape: "record without declaration",
        })?;
        let expression = Expression::call(
            Expression::identifier(Name::new(record.name()).codec_identifier()?),
            Identifier::known("decode"),
            [Expression::identifier(self.reader.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        Ok(match record.is_error_payload() {
            true => ReadExpression::error_record(id, expression),
            false => ReadExpression::dynamic(expression),
        })
    }

    fn enumeration(&self, id: EnumId) -> Result<ReadExpression> {
        let enumeration = self
            .context
            .enumeration(id)
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "enum without declaration",
            })?;
        let expression = Expression::call(
            Expression::identifier(Name::new(enumeration.name()).codec_identifier()?),
            Identifier::known("decode"),
            [Expression::identifier(self.reader.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        Ok(match enumeration.is_error_payload() {
            true => ReadExpression::error_enum(id, expression),
            false => ReadExpression::dynamic(expression),
        })
    }
}

impl ReadExpression {
    pub fn kind(&self) -> Option<ReadKind> {
        self.kind
    }

    pub fn into_expression(self) -> Expression {
        self.expression
    }

    fn string(expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(ReadKind::String),
        }
    }

    fn utf8_string(expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(ReadKind::Utf8String),
        }
    }

    fn bytes(expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(ReadKind::Bytes),
        }
    }

    fn primitive(primitive: Primitive, expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(ReadKind::Primitive(primitive)),
        }
    }

    fn dynamic(expression: Expression) -> Self {
        Self {
            expression,
            kind: None,
        }
    }

    fn error_record(id: RecordId, expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(ReadKind::ErrorRecord(id)),
        }
    }

    fn error_enum(id: EnumId, expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(ReadKind::ErrorEnum(id)),
        }
    }

    fn custom(self) -> Self {
        Self {
            kind: self.kind.map(|kind| match kind {
                ReadKind::Primitive(primitive) => ReadKind::CustomPrimitive(primitive),
                other => other,
            }),
            expression: self.expression,
        }
    }
}

impl CodecRead for Reader<'_> {
    type Expr = Result<ReadExpression>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        Ok(ReadExpression::primitive(
            primitive,
            Expression::call(
                Expression::identifier(self.reader.clone()),
                Scalar::new(primitive)?.read_method(),
                ArgumentList::default(),
            ),
        ))
    }

    fn string(&mut self) -> Self::Expr {
        Ok(ReadExpression::string(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::known("readString"),
            ArgumentList::default(),
        )))
    }

    fn utf8_string(&mut self) -> Self::Expr {
        Ok(ReadExpression::utf8_string(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::known("readString"),
            ArgumentList::default(),
        )))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        // TypeScript does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec read reached TypeScript renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok(ReadExpression::bytes(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::known("readBytes"),
            ArgumentList::default(),
        )))
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        self.record(id)
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        self.record(id)
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        self.enumeration(id)
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        self.enumeration(id)
    }

    fn class_handle(&mut self, _id: ClassId) -> Self::Expr {
        Self::unsupported("class handle codec read")
    }

    fn callback_handle(&mut self, _id: CallbackId) -> Self::Expr {
        Self::unsupported("callback handle codec read")
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        self.context
            .custom_type(id)
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "custom type without declaration",
            })?;
        representation.map(ReadExpression::custom)
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        Ok(ReadExpression::dynamic(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::known(match kind {
                BuiltinType::Duration => "readDuration",
                BuiltinType::SystemTime => "readTimestamp",
                BuiltinType::Uuid => "readUuid",
                BuiltinType::Url => "readUrl",
            }),
            ArgumentList::default(),
        )))
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        let inner = inner?;
        let kind = match inner.kind() {
            Some(ReadKind::Primitive(primitive) | ReadKind::CustomPrimitive(primitive)) => {
                Some(ReadKind::OptionalPrimitive(primitive))
            }
            _ => None,
        };
        Ok(ReadExpression {
            kind,
            expression: Expression::call(
                Expression::identifier(self.reader.clone()),
                Identifier::known("readOptional"),
                [Expression::lambda(inner.into_expression())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        })
    }

    fn sequence(&mut self, _len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        let expression = match element.kind() {
            Some(ReadKind::Primitive(primitive)) => Expression::call(
                Expression::identifier(self.reader.clone()),
                Scalar::new(primitive)?.read_array_method(),
                ArgumentList::default(),
            ),
            Some(ReadKind::CustomPrimitive(primitive)) => Expression::static_call(
                "Array",
                Identifier::known("from"),
                [Expression::call(
                    Expression::identifier(self.reader.clone()),
                    Scalar::new(primitive)?.read_array_method(),
                    ArgumentList::default(),
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            ),
            _ => Expression::call(
                Expression::identifier(self.reader.clone()),
                Identifier::known("readArray"),
                [Expression::lambda(element.into_expression())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        };
        Ok(ReadExpression::dynamic(expression))
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        elements
            .into_iter()
            .map(|element| element.map(ReadExpression::into_expression))
            .collect::<Result<Vec<_>>>()
            .map(Expression::array)
            .map(ReadExpression::dynamic)
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        let err = err?;
        let error = match err.kind() {
            Some(ReadKind::String | ReadKind::Utf8String) => Expression::construct(
                "Error",
                [err.into_expression()]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
            Some(ReadKind::ErrorRecord(id)) => {
                let name = self
                    .context
                    .record(id)
                    .map(|record| Name::new(record.name()).type_name())
                    .ok_or(Error::UnsupportedTarget {
                        target: "typescript",
                        shape: "error record without declaration",
                    })?;
                Expression::construct(
                    format!("{name}Exception"),
                    [err.into_expression()]
                        .into_iter()
                        .collect::<ArgumentList>(),
                )
            }
            Some(ReadKind::ErrorEnum(id)) => {
                let name = self
                    .context
                    .enumeration(id)
                    .map(|enumeration| Name::new(enumeration.name()).type_name())
                    .ok_or(Error::UnsupportedTarget {
                        target: "typescript",
                        shape: "error enum without declaration",
                    })?;
                Expression::construct(
                    format!("{name}Exception"),
                    [err.into_expression()]
                        .into_iter()
                        .collect::<ArgumentList>(),
                )
            }
            _ => Expression::construct(
                "Error",
                [Expression::invoke(
                    Identifier::known("String"),
                    [err.into_expression()]
                        .into_iter()
                        .collect::<ArgumentList>(),
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            ),
        };
        Ok(ReadExpression::dynamic(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::known("readResult"),
            [
                Expression::lambda(ok?.into_expression()),
                Expression::lambda(error),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }

    fn map(&mut self, _kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::dynamic(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::known("readMap"),
            [
                Expression::lambda(key?.into_expression()),
                Expression::lambda(value?.into_expression()),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }
}
