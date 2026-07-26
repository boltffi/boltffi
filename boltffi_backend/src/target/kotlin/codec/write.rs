use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumId,
    MapKind, Native, Op, Primitive, RecordId, ValueRef, WritePlan,
};

use crate::{
    core::{RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::{size::Sizer, value::ValueExpression},
        name_style::Name,
        primitive::KotlinPrimitive,
        render::Enumeration,
        syntax::{ArgumentList, Expression, Identifier, Statement},
        tuple::Arity,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedWrite {
    setup: Vec<Statement>,
    array_argument: Expression,
    direct_arguments: Vec<Expression>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireBuffer {
    buffer: Identifier,
    writer: Identifier,
}

pub struct Writer<'context> {
    writer: Identifier,
    current: Expression,
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteStatement {
    statement: Statement,
    primitive: Option<Primitive>,
}

impl EncodedWrite {
    pub fn new(
        setup: Vec<Statement>,
        array_argument: Expression,
        direct_arguments: Vec<Expression>,
        cleanup: Vec<Statement>,
    ) -> Self {
        Self {
            setup,
            array_argument,
            direct_arguments,
            cleanup,
        }
    }

    pub fn into_array_parts(self) -> (Vec<Statement>, Expression, Vec<Statement>) {
        (self.setup, self.array_argument, self.cleanup)
    }

    pub fn into_direct_parts(self) -> (Vec<Statement>, Vec<Expression>, Vec<Statement>) {
        (self.setup, self.direct_arguments, self.cleanup)
    }
}

impl WireBuffer {
    pub fn new(name: &Name) -> Result<Self> {
        Ok(Self {
            buffer: name.generated("wire")?,
            writer: name.generated("writer")?,
        })
    }

    pub fn writer(&self) -> &Identifier {
        &self.writer
    }

    pub fn write(
        self,
        plan: &WritePlan,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<EncodedWrite> {
        self.write_value(
            plan,
            Expression::identifier(Identifier::parse("value")?),
            host,
            context,
        )
    }

    pub fn write_value(
        self,
        plan: &WritePlan,
        value: Expression,
        host: &KotlinHost,
        context: &RenderContext<Native>,
    ) -> Result<EncodedWrite> {
        let size = plan
            .size_with(&mut Sizer::new(host, context)?.current(value.clone()))?
            .into_expression();
        let mut writer = Writer::new(self.writer.clone(), host, context)?.current(value);
        let writes = plan
            .render_with(&mut writer)
            .into_iter()
            .map(|write| write.map(WriteStatement::into_statement))
            .collect::<Result<Vec<_>>>()?;
        self.write_statements(size, writes)
    }

    pub fn write_statements(
        self,
        size: Expression,
        writes: Vec<Statement>,
    ) -> Result<EncodedWrite> {
        let setup = std::iter::once(Statement::value(
            self.buffer.clone(),
            Expression::call(
                "WireWriterPool",
                Identifier::parse("acquire")?,
                [size].into_iter().collect::<ArgumentList>(),
            ),
        ))
        .chain(std::iter::once(Statement::value(
            self.writer.clone(),
            Expression::property(
                Expression::identifier(self.buffer.clone()),
                Identifier::parse("writer")?,
            ),
        )))
        .chain(writes)
        .collect();
        let cleanup = vec![Statement::expression(Expression::call(
            Expression::identifier(self.buffer.clone()),
            Identifier::parse("close")?,
            ArgumentList::default(),
        ))];
        let buffer = Expression::identifier(self.buffer);
        let array_argument = Expression::call(
            buffer.clone(),
            Identifier::parse("bytes")?,
            ArgumentList::default(),
        );
        let direct_arguments = ["directBuffer", "size"]
            .into_iter()
            .map(|method| {
                Identifier::parse(method)
                    .map(|method| Expression::call(buffer.clone(), method, ArgumentList::default()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(EncodedWrite::new(
            setup,
            array_argument,
            direct_arguments,
            cleanup,
        ))
    }
}

impl<'context> Writer<'context> {
    pub fn new(
        writer: Identifier,
        host: &'context KotlinHost,
        context: &'context RenderContext<'context, Native>,
    ) -> Result<Self> {
        Ok(Self {
            writer,
            current: Expression::identifier(Identifier::parse("value")?),
            host,
            context,
        })
    }

    pub fn current(mut self, current: Expression) -> Self {
        self.current = current;
        self
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value)?
            .current(self.current.clone())
            .render()
    }

    fn writer_call(&self, method: Identifier, value: &ValueRef) -> Result<Statement> {
        self.writer_call_expression(method, self.value(value)?)
    }

    fn writer_call_expression(&self, method: Identifier, value: Expression) -> Result<Statement> {
        Ok(Statement::expression(Expression::call(
            Expression::identifier(self.writer.clone()),
            method,
            [value].into_iter().collect::<ArgumentList>(),
        )))
    }

    fn writer_call_fragment(&self, method: Identifier, value: &ValueRef) -> Result<WriteStatement> {
        self.writer_call(method, value).map(WriteStatement::new)
    }

    fn primitive_method(primitive: Primitive) -> Result<Identifier> {
        KotlinPrimitive::new(primitive)
            .wire_method_suffix()
            .and_then(|suffix| Identifier::parse(format!("write{suffix}")))
    }

    fn native_primitive_method(primitive: Primitive) -> Result<Identifier> {
        KotlinPrimitive::new(primitive)
            .native_wire_method_suffix()
            .and_then(|suffix| Identifier::parse(format!("write{suffix}")))
    }

    fn unsupported(shape: &'static str) -> Vec<Result<WriteStatement>> {
        vec![Err(KotlinHost::unsupported(shape))]
    }

    fn single_statement(statements: Vec<Result<WriteStatement>>) -> Result<Statement> {
        let mut statements = statements.into_iter().collect::<Result<Vec<_>>>()?;
        match statements.len() {
            1 => Ok(statements.remove(0).into_statement()),
            _ => Err(KotlinHost::unsupported("multi-statement wire writer")),
        }
    }

    fn with_current(
        &mut self,
        current: Expression,
        render: impl FnOnce(&mut Self, &ValueRef) -> Vec<Result<WriteStatement>>,
    ) -> Vec<Result<WriteStatement>> {
        let previous = std::mem::replace(&mut self.current, current);
        let statements = render(self, &ValueRef::self_value());
        self.current = previous;
        statements
    }
}

impl WriteStatement {
    pub fn into_statement(self) -> Statement {
        self.statement
    }

    fn new(statement: Statement) -> Self {
        Self {
            statement,
            primitive: None,
        }
    }

    fn primitive(primitive: Primitive, statement: Statement) -> Self {
        Self {
            statement,
            primitive: Some(primitive),
        }
    }

    fn without_primitive(mut self) -> Self {
        self.primitive = None;
        self
    }
}

impl CodecWrite for Writer<'_> {
    type Stmt = Result<WriteStatement>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![Self::primitive_method(primitive).and_then(|method| {
            self.writer_call(method, value)
                .map(|statement| WriteStatement::primitive(primitive, statement))
        })]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            Identifier::parse("writeString")
                .and_then(|method| self.writer_call_fragment(method, value)),
        ]
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Vec<Self::Stmt> {
        // Kotlin does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec write reached Kotlin renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            Identifier::parse("writeBytes")
                .and_then(|method| self.writer_call_fragment(method, value)),
        ]
    }

    fn direct_record(&mut self, _id: RecordId, _value: &ValueRef) -> Vec<Self::Stmt> {
        Self::unsupported("direct-record wire write")
    }

    fn encoded_record(&mut self, _id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    value,
                    Identifier::parse("writeTo")?,
                    [Expression::identifier(self.writer.clone())]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            )))
        })]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            Enumeration::from_id(id, self.host, self.context).and_then(|enumeration| {
                Self::native_primitive_method(enumeration.repr()?).and_then(|method| {
                    self.writer_call_expression(
                        method,
                        Expression::property(self.value(value)?, Identifier::parse("value")?),
                    )
                    .map(WriteStatement::new)
                })
            }),
        ]
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Enumeration::write_statement(id, value, self.writer.clone(), self.context)
                .map(WriteStatement::new)
        })]
    }

    fn class_handle(&mut self, _id: ClassId, _value: &ValueRef) -> Vec<Self::Stmt> {
        Self::unsupported("class handle wire write")
    }

    fn callback_handle(&mut self, _id: CallbackId, _value: &ValueRef) -> Vec<Self::Stmt> {
        Self::unsupported("callback handle wire write")
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
            Some(mapping) => match self
                .value(value)
                .and_then(|value| KotlinHost::custom_type_encode(mapping, value))
            {
                Ok(value) => self.with_current(value, representation),
                Err(error) => vec![Err(error)],
            },
            None => representation(self, value),
        };
        representation
            .into_iter()
            .map(|statement| statement.map(WriteStatement::without_primitive))
            .collect()
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        let method = match kind {
            BuiltinType::Duration => "writeDuration",
            BuiltinType::SystemTime => "writeInstant",
            BuiltinType::Uuid => "writeUuid",
            BuiltinType::Url => "writeUri",
        };
        vec![Identifier::parse(method).and_then(|method| self.writer_call_fragment(method, value))]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::parse("writeOptionalValue")?,
                    [
                        value,
                        Expression::lambda_statement(
                            vec![self.writer.clone(), ValueExpression::binder(binder)?],
                            Self::single_statement(inner)?,
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
        vec![self.value(value).and_then(|sequence| {
            if let [Ok(element)] = element.as_slice()
                && let Some(primitive) = element.primitive
            {
                return KotlinPrimitive::new(primitive)
                    .wire_array_method_suffix()
                    .and_then(|suffix| Identifier::parse(format!("write{suffix}Array")))
                    .and_then(|method| self.writer_call_expression(method, sequence))
                    .map(WriteStatement::new);
            }
            let count = Expression::property(sequence.clone(), Identifier::parse("size")?);
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::parse("writeSequence")?,
                    [
                        sequence,
                        count,
                        Expression::lambda_statement(
                            vec![self.writer.clone(), ValueExpression::binder(binder)?],
                            Self::single_statement(element)?,
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }

    fn tuple(&mut self, _value: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        match Arity::from_count(elements.len()) {
            Ok(_) => elements.into_iter().flatten().collect(),
            Err(error) => vec![Err(error)],
        }
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::parse("writeResult")?,
                    [
                        value,
                        Expression::lambda_statement(
                            vec![self.writer.clone(), ValueExpression::binder(binder)?],
                            Self::single_statement(ok)?,
                        ),
                        Expression::lambda_statement(
                            vec![self.writer.clone(), ValueExpression::binder(binder)?],
                            Self::single_statement(err)?,
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
        _kind: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|map| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::call(
                    Expression::identifier(self.writer.clone()),
                    Identifier::parse("writeMap")?,
                    [
                        map,
                        Expression::lambda_statement(
                            vec![self.writer.clone(), ValueExpression::binder(key_binder)?],
                            Self::single_statement(key)?,
                        ),
                        Expression::lambda_statement(
                            vec![self.writer.clone(), ValueExpression::binder(value_binder)?],
                            Self::single_statement(map_value)?,
                        ),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                ),
            )))
        })]
    }
}
