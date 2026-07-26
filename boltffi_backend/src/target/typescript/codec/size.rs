use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecSize, CustomTypeId, ElementCount, EnumDecl,
    EnumId, MapKind, Op, Primitive, RecordId, ValueRef, Wasm32,
};

use crate::core::{Error, RenderContext, Result};

use super::super::{
    name_style::Name,
    syntax::{ArgumentList, Expression, Identifier},
};
use super::operation::Operation;
use super::value::ValueExpression;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SizeKind {
    Primitive(Primitive),
    String,
    Utf8String,
    Bytes,
}

pub struct SizeExpression {
    expression: Expression,
    kind: Option<SizeKind>,
}

pub struct Sizer<'context> {
    current: Expression,
    context: &'context RenderContext<'context, Wasm32>,
}

impl<'context> Sizer<'context> {
    pub fn new(current: Expression, context: &'context RenderContext<'context, Wasm32>) -> Self {
        Self { current, context }
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value, self.current.clone()).render()
    }

    fn unsupported(shape: &'static str) -> Result<SizeExpression> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }

    fn record_size(&self, id: RecordId, value: &ValueRef) -> Result<SizeExpression> {
        let codec = self
            .context
            .record(id)
            .map(|record| Name::new(record.name()).codec_identifier())
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "record without declaration",
            })??;
        Ok(SizeExpression::dynamic(Expression::call(
            Expression::identifier(codec),
            Identifier::known("size"),
            [self.value(value)?].into_iter().collect::<ArgumentList>(),
        )))
    }

    fn enum_size(&self, id: EnumId, value: &ValueRef) -> Result<SizeExpression> {
        let enumeration = self
            .context
            .enumeration(id)
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "enum without declaration",
            })?;
        match enumeration {
            EnumDecl::CStyle(enumeration) => Ok(SizeExpression::dynamic(Expression::integer(
                enumeration.repr().primitive().byte_size::<Wasm32>().get(),
            ))),
            EnumDecl::Data(enumeration) => Ok(SizeExpression::dynamic(Expression::call(
                Expression::identifier(Name::new(enumeration.name()).codec_identifier()?),
                Identifier::known("size"),
                [self.value(value)?].into_iter().collect::<ArgumentList>(),
            ))),
            _ => Self::unsupported("unknown enum declaration"),
        }
    }
}

impl SizeExpression {
    pub fn kind(&self) -> Option<SizeKind> {
        self.kind
    }

    pub fn into_expression(self) -> Expression {
        self.expression
    }

    fn string(expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(SizeKind::String),
        }
    }

    fn utf8_string() -> Self {
        Self {
            expression: Expression::integer(0),
            kind: Some(SizeKind::Utf8String),
        }
    }

    fn bytes(expression: Expression) -> Self {
        Self {
            expression,
            kind: Some(SizeKind::Bytes),
        }
    }

    fn primitive(primitive: Primitive) -> Self {
        Self {
            expression: Expression::integer(primitive.byte_size::<Wasm32>().get()),
            kind: Some(SizeKind::Primitive(primitive)),
        }
    }

    fn dynamic(expression: Expression) -> Self {
        Self {
            expression,
            kind: None,
        }
    }
}

impl CodecSize for Sizer<'_> {
    type Expr = Result<SizeExpression>;

    fn primitive(&mut self, primitive: Primitive, _value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::primitive(primitive))
    }

    fn string(&mut self, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::string(Expression::invoke(
            Identifier::known("wireStringSize"),
            [self.value(value)?].into_iter().collect::<ArgumentList>(),
        )))
    }

    fn utf8_string(&mut self, _value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::utf8_string())
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Self::Expr {
        // TypeScript does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec size reached TypeScript renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::bytes(Expression::integer(4).add(
            Expression::property(self.value(value)?, Identifier::known("length")),
        )))
    }

    fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr {
        self.record_size(id, value)
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr {
        self.record_size(id, value)
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr {
        self.enum_size(id, value)
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr {
        self.enum_size(id, value)
    }

    fn class_handle(&mut self, _id: ClassId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("class handle codec size")
    }

    fn callback_handle(&mut self, _id: CallbackId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("callback handle codec size")
    }

    fn custom<F>(&mut self, id: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
    where
        F: FnOnce(&mut Self, &ValueRef) -> Self::Expr,
    {
        self.context
            .custom_type(id)
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "custom type without declaration",
            })?;
        representation(self, value)
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
        match kind {
            BuiltinType::Duration | BuiltinType::SystemTime => {
                Ok(SizeExpression::dynamic(Expression::integer(12)))
            }
            BuiltinType::Uuid => Ok(SizeExpression::dynamic(Expression::integer(16))),
            BuiltinType::Url => Ok(SizeExpression::dynamic(Expression::invoke(
                Identifier::known("wireStringSize"),
                [self.value(value)?].into_iter().collect::<ArgumentList>(),
            ))),
        }
    }

    fn optional(&mut self, value: &ValueRef, binder: BinderId, inner: Self::Expr) -> Self::Expr {
        Ok(SizeExpression::dynamic(Expression::invoke(
            Identifier::known("wireOptionalSize"),
            [
                self.value(value)?,
                Expression::parameter_lambda(
                    ValueExpression::binder(binder)?,
                    inner?.into_expression(),
                ),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        len: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr {
        let element = element?;
        match element.kind() {
            Some(SizeKind::Primitive(primitive)) => Ok(SizeExpression::dynamic(
                Expression::integer(4).add(
                    len.render_with(&mut Operation::new(self.current.clone(), value)?)?
                        .multiply(Expression::integer(primitive.byte_size::<Wasm32>().get())),
                ),
            )),
            _ => Ok(SizeExpression::dynamic(Expression::invoke(
                Identifier::known("wireArraySize"),
                [
                    self.value(value)?,
                    Expression::parameter_lambda(
                        ValueExpression::binder(binder)?,
                        element.into_expression(),
                    ),
                ]
                .into_iter()
                .collect::<ArgumentList>(),
            ))),
        }
    }

    fn tuple(&mut self, _value: &ValueRef, _elements: Vec<Self::Expr>) -> Self::Expr {
        Self::unsupported("tuple codec size")
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr {
        let binder = ValueExpression::binder(binder)?;
        Ok(SizeExpression::dynamic(Expression::invoke(
            Identifier::known("wireResultSize"),
            [
                self.value(value)?,
                Expression::parameter_lambda(binder.clone(), ok?.into_expression()),
                Expression::parameter_lambda(binder, err?.into_expression()),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }

    fn map(
        &mut self,
        kind: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Self::Expr,
        value_binder: BinderId,
        map_value: Self::Expr,
    ) -> Self::Expr {
        match kind {
            MapKind::Hash => Ok(SizeExpression::dynamic(Expression::invoke(
                Identifier::known("wireMapSize"),
                [
                    self.value(value)?,
                    Expression::parameter_lambda(
                        ValueExpression::binder(key_binder)?,
                        key?.into_expression(),
                    ),
                    Expression::parameter_lambda(
                        ValueExpression::binder(value_binder)?,
                        map_value?.into_expression(),
                    ),
                ]
                .into_iter()
                .collect::<ArgumentList>(),
            ))),
            MapKind::BTree => Self::unsupported("BTreeMap codec size"),
        }
    }
}
