use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumDecl,
    EnumId, MapKind, Native, Op, Primitive, RecordId, ValueRef,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::swift::{
        SwiftHost,
        codec::value::{ValueExpression, ValueScope},
        primitive::SwiftPrimitive,
        syntax::{ArgumentList, Expression, Identifier, Statement},
    },
};

pub struct Writer<'context, 'bindings> {
    name: Identifier,
    scope: ValueScope,
    context: &'context RenderContext<'bindings, Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteStatement {
    statement: Statement,
    value: WriteValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteValue {
    String,
    Other,
}

impl<'context, 'bindings> Writer<'context, 'bindings> {
    pub fn new(
        name: Identifier,
        scope: impl Into<ValueScope>,
        context: &'context RenderContext<'bindings, Native>,
    ) -> Self {
        Self {
            name,
            scope: scope.into(),
            context,
        }
    }

    fn unsupported<T>(&self, shape: &'static str) -> Result<T> {
        Err(SwiftHost::unsupported(shape))
    }

    fn write(&self, method: &str, value: &ValueRef) -> Result<WriteStatement> {
        Ok(WriteStatement::new(Statement::expression(
            Expression::call(
                Expression::member(&self.name, method),
                [ValueExpression::new(value, self.scope.clone()).render()?]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        )))
    }

    fn write_encodable(&self, value: &ValueRef) -> Result<WriteStatement> {
        ValueExpression::new(value, self.scope.clone())
            .render()
            .map(|value| {
                WriteStatement::new(Statement::expression(Expression::call(
                    Expression::member(value, "encode"),
                    [Expression::labeled("to", Expression::address(&self.name))]
                        .into_iter()
                        .collect::<ArgumentList>(),
                )))
            })
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value, self.scope.clone()).render()
    }

    fn with_value<F>(&mut self, value: Expression, representation: F) -> Vec<Result<WriteStatement>>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Result<WriteStatement>>,
    {
        let mut writer = Self::new(self.name.clone(), value, self.context);
        representation(&mut writer, &ValueRef::self_value())
    }

    fn single_statement(statements: Vec<Result<WriteStatement>>) -> Result<WriteStatement> {
        let statements = statements.into_iter().collect::<Result<Vec<_>>>()?;
        match statements.as_slice() {
            [statement] => Ok(statement.clone()),
            _ => Err(SwiftHost::unsupported("multi-statement codec write")),
        }
    }

    fn c_style_enum_repr(&self, id: EnumId) -> Result<Primitive> {
        match self.context.enumeration(id) {
            Some(EnumDecl::CStyle(enumeration)) => Ok(enumeration.repr().primitive()),
            Some(EnumDecl::Data(_)) => Err(SwiftHost::unsupported(
                "data enum where C-style enum was expected",
            )),
            Some(_) => Err(SwiftHost::unsupported(
                "unknown enum where C-style enum was expected",
            )),
            None => Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing enum type in Swift codec writer",
            }),
        }
    }
}

impl CodecWrite for Writer<'_, '_> {
    type Stmt = Result<WriteStatement>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            SwiftPrimitive::new(primitive)
                .write_statement(self.name.clone(), value)
                .map(WriteStatement::new)
        })]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("writeString", value).map(WriteStatement::string)]
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Vec<Self::Stmt> {
        // Swift does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec write reached Swift renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write("writeBytes", value)]
    }

    fn direct_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_encodable(value)]
    }

    fn encoded_record(&mut self, _: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_encodable(value)]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            SwiftPrimitive::new(self.c_style_enum_repr(id)?)
                .write_statement(self.name.clone(), Expression::member(value, "rawValue"))
                .map(WriteStatement::new)
        })]
    }

    fn data_enum(&mut self, _: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.write_encodable(value)]
    }

    fn class_handle(&mut self, _: ClassId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.unsupported("class handle codec write")]
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.unsupported("callback handle codec write")]
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
        let representation = match self.context.custom_type_mapping(id) {
            Some(mapping) => match self.value(value) {
                Ok(value) => self.with_value(
                    SwiftHost::custom_type_encode(mapping, value),
                    representation,
                ),
                Err(error) => vec![Err(error)],
            },
            None => representation(self, value),
        };
        representation
            .into_iter()
            .map(|statement| statement.map(WriteStatement::custom_representation))
            .collect()
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        let method = match kind {
            BuiltinType::Duration => "writeDuration",
            BuiltinType::SystemTime => "writeTimestamp",
            BuiltinType::Uuid => "writeUuid",
            BuiltinType::Url => "writeUrl",
        };
        vec![self.write(method, value)]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::trailing_closure_parameters(
                    Expression::member(self.name.clone(), "writeOptional"),
                    [value].into_iter().collect::<ArgumentList>(),
                    [self.name.clone(), ValueExpression::binder(binder)?],
                    Self::single_statement(inner)?.statement,
                ),
            )))
        })]
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::trailing_closure_parameters(
                    Expression::member(self.name.clone(), "writeArray"),
                    [value].into_iter().collect::<ArgumentList>(),
                    [self.name.clone(), ValueExpression::binder(binder)?],
                    Self::single_statement(element)?.statement,
                ),
            )))
        })]
    }

    fn tuple(&mut self, _: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        elements.into_iter().flatten().collect()
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            let ok = Self::single_statement(ok)?.statement;
            let err = Self::single_statement(err)?;
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    Expression::member(self.name.clone(), "writeResult"),
                    [
                        value,
                        Expression::closure(
                            [self.name.clone(), ValueExpression::binder(binder)?],
                            ok,
                        ),
                        Expression::closure(
                            [self.name.clone(), ValueExpression::binder(binder)?],
                            err.result_error(self.name.clone(), binder)?,
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }

    fn map(
        &mut self,
        _: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    Expression::member(self.name.clone(), "writeMap"),
                    [
                        value,
                        Expression::closure(
                            [self.name.clone(), ValueExpression::binder(key_binder)?],
                            Self::single_statement(key)?.statement,
                        ),
                        Expression::closure(
                            [self.name.clone(), ValueExpression::binder(value_binder)?],
                            Self::single_statement(map_value)?.statement,
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }
}

impl WriteStatement {
    pub fn into_statement(self) -> Statement {
        self.statement
    }

    fn new(statement: Statement) -> Self {
        Self {
            statement,
            value: WriteValue::Other,
        }
    }

    fn string(mut self) -> Self {
        self.value = WriteValue::String;
        self
    }

    fn custom_representation(mut self) -> Self {
        self.value = WriteValue::Other;
        self
    }

    fn result_error(&self, writer: Identifier, binder: BinderId) -> Result<Statement> {
        match self.value {
            WriteValue::String => Ok(Statement::expression(Expression::call(
                Expression::member(writer, "writeString"),
                [Expression::member(
                    ValueExpression::binder(binder)?,
                    "message",
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            ))),
            WriteValue::Other => Ok(self.statement.clone()),
        }
    }
}
