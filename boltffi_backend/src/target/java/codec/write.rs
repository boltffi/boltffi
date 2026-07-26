use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumId,
    MapKind, Op, Primitive as BindingPrimitive, RecordId, ValueRef, WritePlan,
};

use crate::{
    core::{RenderContext, Result},
    target::java::{
        JavaHost, JavaVersion,
        codec::{
            SequenceElement,
            size::Sizer,
            value::{ValueExpression, ValueMemberAccess},
        },
        primitive::Primitive,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeIdentifier, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedWrite {
    lease: Identifier,
    acquire: Vec<Statement>,
    prepare: Vec<Statement>,
    arguments: Vec<Expression>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireBuffer {
    lease: Identifier,
    writer: Identifier,
    version: JavaVersion,
}

pub struct Writer<'context> {
    writer: Identifier,
    current: Expression,
    member_access: ValueMemberAccess,
    version: JavaVersion,
    context: &'context RenderContext<'context, boltffi_binding::Native>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteStatement {
    statement: Statement,
    sequence_element: SequenceElement,
}

impl EncodedWrite {
    pub fn into_parts(
        self,
    ) -> (
        Vec<Statement>,
        Vec<Statement>,
        Vec<Expression>,
        Vec<Statement>,
    ) {
        (self.acquire, self.prepare, self.arguments, self.cleanup)
    }

    pub fn into_bytes_parts(self) -> (Vec<Statement>, Vec<Statement>, Expression, Vec<Statement>) {
        (
            self.acquire,
            self.prepare,
            Expression::identifier(self.lease)
                .call(Identifier::known("bytes"), ArgumentList::default()),
            self.cleanup,
        )
    }
}

impl WireBuffer {
    pub fn new(name: &crate::target::java::name_style::Name, version: JavaVersion) -> Result<Self> {
        Ok(Self {
            lease: name.generated("wire", version)?,
            writer: name.generated("writer", version)?,
            version,
        })
    }

    pub fn receiver(version: JavaVersion) -> Result<Self> {
        Ok(Self {
            lease: Identifier::parse_for("__boltffi_receiver_wire", version)?,
            writer: Identifier::parse_for("__boltffi_receiver_writer", version)?,
            version,
        })
    }

    pub fn write<'context>(
        self,
        plan: &WritePlan,
        value: Expression,
        context: &'context RenderContext<'context, boltffi_binding::Native>,
    ) -> Result<EncodedWrite> {
        let size = plan
            .size_with(&mut Sizer::new(self.version, context).current(value.clone()))?
            .into_expression();
        let writes = plan
            .render_with(
                &mut Writer::new(self.writer.clone(), self.version, context).current(value),
            )
            .into_iter()
            .map(|statement| statement.map(WriteStatement::into_statement))
            .collect::<Result<Vec<_>>>()?;
        self.write_statements(size, writes)
    }

    pub fn write_statements(
        self,
        size: Expression,
        writes: Vec<Statement>,
    ) -> Result<EncodedWrite> {
        let lease_type = TypeName::named(TypeIdentifier::known("WireLease", self.version));
        let writer_type = TypeName::named(TypeIdentifier::known("WireWriter", self.version));
        let lease = Expression::identifier(self.lease.clone());
        let acquire = vec![Statement::value(
            lease_type,
            self.lease.clone(),
            Expression::static_call(
                TypeName::named(TypeIdentifier::known("WireWriterPool", self.version)),
                Identifier::known("acquire"),
                [size].into_iter().collect(),
            ),
        )];
        let prepare = std::iter::once(Statement::value(
            writer_type,
            self.writer,
            lease
                .clone()
                .call(Identifier::known("writer"), ArgumentList::default()),
        ))
        .chain(writes)
        .collect();
        let arguments = ["directBuffer", "size"]
            .into_iter()
            .map(|method| {
                lease
                    .clone()
                    .call(Identifier::known(method), ArgumentList::default())
            })
            .collect();
        let cleanup = vec![Statement::expression(
            lease.call(Identifier::known("close"), ArgumentList::default()),
        )];
        Ok(EncodedWrite {
            lease: self.lease,
            acquire,
            prepare,
            arguments,
            cleanup,
        })
    }
}

impl<'context> Writer<'context> {
    pub fn new(
        writer: Identifier,
        version: JavaVersion,
        context: &'context RenderContext<'context, boltffi_binding::Native>,
    ) -> Self {
        Self {
            writer,
            current: Expression::identifier(Identifier::known("value")),
            member_access: ValueMemberAccess::Accessor,
            version,
            context,
        }
    }

    pub fn current(mut self, current: Expression) -> Self {
        self.current = current;
        self
    }

    pub fn members(mut self, access: ValueMemberAccess) -> Self {
        self.member_access = access;
        self
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value, self.version)
            .current(self.current.clone())
            .member_access(self.member_access)
            .render()
    }

    fn call(&self, method: Identifier, value: Expression) -> Statement {
        Statement::expression(
            Expression::identifier(self.writer.clone()).call(method, [value].into_iter().collect()),
        )
    }

    fn unsupported(shape: &'static str) -> Vec<Result<WriteStatement>> {
        vec![Err(JavaHost::unsupported(shape))]
    }

    fn single(statements: Vec<Result<WriteStatement>>) -> Result<Statement> {
        let mut statements = statements.into_iter().collect::<Result<Vec<_>>>()?;
        match statements.len() {
            1 => Ok(statements.remove(0).statement),
            _ => Err(JavaHost::unsupported("multi-statement wire writer")),
        }
    }
}

impl WriteStatement {
    pub fn into_statement(self) -> Statement {
        self.statement
    }

    fn new(statement: Statement) -> Self {
        Self {
            statement,
            sequence_element: SequenceElement::General,
        }
    }

    fn primitive(primitive: Primitive, statement: Statement) -> Self {
        Self {
            statement,
            sequence_element: SequenceElement::Primitive(primitive),
        }
    }

    fn string(statement: Statement) -> Self {
        Self {
            statement,
            sequence_element: SequenceElement::String,
        }
    }
}

impl CodecWrite for Writer<'_> {
    type Stmt = Result<WriteStatement>;

    fn primitive(&mut self, primitive: BindingPrimitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![Primitive::try_from(primitive).and_then(|primitive| {
            Ok(WriteStatement::primitive(
                primitive,
                self.call(
                    Identifier::parse_for(
                        format!("write{}", primitive.wire_method_suffix()),
                        self.version,
                    )?,
                    self.value(value)?,
                ),
            ))
        })]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            WriteStatement::string(self.call(Identifier::known("writeString"), value))
        })]
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Vec<Self::Stmt> {
        // Java does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec write reached Java renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value).map(|value| {
                WriteStatement::new(self.call(Identifier::known("writeBytes"), value))
            }),
        ]
    }

    fn direct_record(&mut self, _id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            WriteStatement::new(Statement::expression(
                value.call(
                    Identifier::known("writeTo"),
                    [Expression::identifier(self.writer.clone())]
                        .into_iter()
                        .collect(),
                ),
            ))
        })]
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.direct_record(id, value)
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            crate::target::java::render::Enumeration::c_style_primitive(id, self.context).and_then(
                |primitive| {
                    Identifier::parse_for(
                        format!("write{}", primitive.wire_method_suffix()),
                        self.version,
                    )
                    .and_then(|method| {
                        self.value(value).map(|value| {
                            WriteStatement::new(self.call(
                                method,
                                value.call(
                                    Identifier::known("nativeValue"),
                                    ArgumentList::default(),
                                ),
                            ))
                        })
                    })
                },
            ),
        ]
    }

    fn data_enum(&mut self, _id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).map(|value| {
            WriteStatement::new(Statement::expression(
                value.call(
                    Identifier::known("writeTo"),
                    [Expression::identifier(self.writer.clone())]
                        .into_iter()
                        .collect(),
                ),
            ))
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
        _id: CustomTypeId,
        value: &ValueRef,
        representation: F,
    ) -> Vec<Self::Stmt>
    where
        F: FnOnce(&mut Self, &ValueRef) -> Vec<Self::Stmt>,
    {
        representation(self, value)
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        let method = match kind {
            BuiltinType::Duration => "writeDuration",
            BuiltinType::SystemTime => "writeInstant",
            BuiltinType::Uuid => "writeUuid",
            BuiltinType::Url => "writeUri",
        };
        vec![
            self.value(value)
                .map(|value| WriteStatement::new(self.call(Identifier::known(method), value))),
        ]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(WriteStatement::new(Statement::expression(
                Expression::identifier(self.writer.clone()).call(
                    Identifier::known("writeOptional"),
                    [
                        value,
                        Expression::lambda_statement(
                            [ValueExpression::binder(binder, self.version)?],
                            Self::single(inner)?,
                        ),
                    ]
                    .into_iter()
                    .collect(),
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
            if let [Ok(element)] = element.as_slice() {
                let method = match element.sequence_element {
                    SequenceElement::Primitive(primitive) => Some(Identifier::parse_for(
                        format!("write{}Array", primitive.wire_method_suffix()),
                        self.version,
                    )?),
                    SequenceElement::String => Some(Identifier::known("writeStringSequence")),
                    SequenceElement::General | SequenceElement::Fixed(_) => None,
                };
                if let Some(method) = method {
                    return Ok(WriteStatement::new(self.call(method, value)));
                }
            }
            Ok(WriteStatement::new(Statement::expression(
                Expression::identifier(self.writer.clone()).call(
                    Identifier::known("writeSequence"),
                    [
                        value,
                        Expression::lambda_statement(
                            [ValueExpression::binder(binder, self.version)?],
                            Self::single(element)?,
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            )))
        })]
    }

    fn tuple(&mut self, _value: &ValueRef, _elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        Self::unsupported("tuple wire write")
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            let binder = ValueExpression::binder(binder, self.version)?;
            Ok(WriteStatement::new(Statement::expression(
                value.call(
                    Identifier::known("writeTo"),
                    [
                        Expression::identifier(self.writer.clone()),
                        Expression::lambda_statement([binder.clone()], Self::single(ok)?),
                        Expression::lambda_statement([binder], Self::single(err)?),
                    ]
                    .into_iter()
                    .collect(),
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
                Expression::identifier(self.writer.clone()).call(
                    Identifier::known("writeMap"),
                    [
                        map,
                        Expression::lambda_statement(
                            [ValueExpression::binder(key_binder, self.version)?],
                            Self::single(key)?,
                        ),
                        Expression::lambda_statement(
                            [ValueExpression::binder(value_binder, self.version)?],
                            Self::single(map_value)?,
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            )))
        })]
    }
}
