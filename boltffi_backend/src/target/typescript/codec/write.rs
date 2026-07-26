use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumId,
    MapKind, Op, Primitive, RecordId, ValueRef, Wasm32,
};

use crate::core::{Error, RenderContext, Result};

use super::super::{
    name_style::Name,
    primitive::Scalar,
    syntax::{ArgumentList, Expression, Identifier, Statement},
};
use super::value::ValueExpression;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WriteKind {
    Primitive(Primitive),
    String,
    Bytes,
}

pub struct WriteStatement {
    statement: Statement,
    kind: Option<WriteKind>,
}

pub struct Writer<'context> {
    writer: Identifier,
    current: Expression,
    context: &'context RenderContext<'context, Wasm32>,
}

impl<'context> Writer<'context> {
    pub fn new(
        writer: Identifier,
        current: Expression,
        context: &'context RenderContext<'context, Wasm32>,
    ) -> Self {
        Self {
            writer,
            current,
            context,
        }
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value, self.current.clone()).render()
    }

    fn statements(fragments: Vec<Result<WriteStatement>>) -> Result<Vec<Statement>> {
        fragments
            .into_iter()
            .map(|fragment| fragment.map(WriteStatement::into_statement))
            .collect()
    }

    fn unsupported(shape: &'static str) -> Vec<Result<WriteStatement>> {
        vec![Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })]
    }

    fn record(&self, id: RecordId, value: &ValueRef) -> Result<WriteStatement> {
        let codec = self
            .context
            .record(id)
            .map(|record| Name::new(record.name()).codec_identifier())
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "record without declaration",
            })??;
        Ok(WriteStatement::dynamic(Statement::expression(
            Expression::call(
                Expression::identifier(codec),
                Identifier::known("encode"),
                [
                    Expression::identifier(self.writer.clone()),
                    self.value(value)?,
                ]
                .into_iter()
                .collect::<ArgumentList>(),
            ),
        )))
    }

    fn enumeration(&self, id: EnumId, value: &ValueRef) -> Result<WriteStatement> {
        let codec = self
            .context
            .enumeration(id)
            .map(|enumeration| Name::new(enumeration.name()).codec_identifier())
            .ok_or(Error::UnsupportedTarget {
                target: "typescript",
                shape: "enum without declaration",
            })??;
        Ok(WriteStatement::dynamic(Statement::expression(
            Expression::call(
                Expression::identifier(codec),
                Identifier::known("encode"),
                [
                    Expression::identifier(self.writer.clone()),
                    self.value(value)?,
                ]
                .into_iter()
                .collect::<ArgumentList>(),
            ),
        )))
    }
}

impl WriteStatement {
    pub fn kind(&self) -> Option<WriteKind> {
        self.kind
    }

    pub fn into_statement(self) -> Statement {
        self.statement
    }

    fn string(statement: Statement) -> Self {
        Self {
            statement,
            kind: Some(WriteKind::String),
        }
    }

    fn bytes(statement: Statement) -> Self {
        Self {
            statement,
            kind: Some(WriteKind::Bytes),
        }
    }

    fn primitive(primitive: Primitive, statement: Statement) -> Self {
        Self {
            statement,
            kind: Some(WriteKind::Primitive(primitive)),
        }
    }

    fn dynamic(statement: Statement) -> Self {
        Self {
            statement,
            kind: None,
        }
    }
}

impl CodecWrite for Writer<'_> {
    type Stmt = Result<WriteStatement>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::primitive(
                primitive,
                Statement::expression(Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Scalar::new(primitive)?.write_method(),
                    [value].into_iter().collect::<ArgumentList>(),
                )),
            ))
        })]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            WriteStatement::string(Statement::expression(Expression::call(
                Expression::identifier(self.writer.clone()),
                Identifier::known("writeString"),
                [value].into_iter().collect::<ArgumentList>(),
            )))
        })]
    }

    fn utf8_string(&mut self, _value: &ValueRef) -> Vec<Self::Stmt> {
        Vec::new()
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Vec<Self::Stmt> {
        // TypeScript does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec write reached TypeScript renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            WriteStatement::bytes(Statement::expression(Expression::call(
                Expression::identifier(self.writer.clone()),
                Identifier::known("writeBytes"),
                [value].into_iter().collect::<ArgumentList>(),
            )))
        })]
    }

    fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.record(id, value)]
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.record(id, value)]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.enumeration(id, value)]
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.enumeration(id, value)]
    }

    fn class_handle(&mut self, _id: ClassId, _value: &ValueRef) -> Vec<Self::Stmt> {
        Self::unsupported("class handle codec write")
    }

    fn callback_handle(&mut self, _id: CallbackId, _value: &ValueRef) -> Vec<Self::Stmt> {
        Self::unsupported("callback handle codec write")
    }

    fn custom<F>(
        &mut self,
        id: CustomTypeId,
        value: &ValueRef,
        representation: F,
    ) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
        match self.context.custom_type(id) {
            Some(_) => representation(self, value),
            None => Self::unsupported("custom type without declaration"),
        }
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            WriteStatement::dynamic(Statement::expression(Expression::call(
                Expression::identifier(self.writer.clone()),
                Identifier::known(match kind {
                    BuiltinType::Duration => "writeDuration",
                    BuiltinType::SystemTime => "writeTimestamp",
                    BuiltinType::Uuid => "writeUuid",
                    BuiltinType::Url => "writeUrl",
                }),
                [value].into_iter().collect::<ArgumentList>(),
            )))
        })]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::dynamic(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::known("writeOptional"),
                    [
                        value,
                        Expression::statements_lambda(
                            ValueExpression::binder(binder)?,
                            Self::statements(inner)?,
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _len: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::dynamic(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::known("writeArray"),
                    [
                        value,
                        Expression::statements_lambda(
                            ValueExpression::binder(binder)?,
                            Self::statements(element)?,
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }

    fn tuple(&mut self, _value: &ValueRef, _elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        Self::unsupported("tuple codec write")
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            let binder = ValueExpression::binder(binder)?;
            Ok(WriteStatement::dynamic(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::known("writeResult"),
                    [
                        value,
                        Expression::statements_lambda(binder.clone(), Self::statements(ok)?),
                        Expression::statements_lambda(binder, Self::statements(err)?),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }

    fn map(
        &mut self,
        kind: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        match kind {
            MapKind::Hash => vec![self.value(value).and_then(|value| {
                Ok(WriteStatement::dynamic(Statement::expression(
                    Expression::call(
                        Expression::identifier(self.writer.clone()),
                        Identifier::known("writeMap"),
                        [
                            value,
                            Expression::statements_lambda(
                                ValueExpression::binder(key_binder)?,
                                Self::statements(key)?,
                            ),
                            Expression::statements_lambda(
                                ValueExpression::binder(value_binder)?,
                                Self::statements(map_value)?,
                            ),
                        ]
                        .into_iter()
                        .collect::<ArgumentList>(),
                    ),
                )))
            })],
            MapKind::BTree => Self::unsupported("BTreeMap codec write"),
        }
    }
}
