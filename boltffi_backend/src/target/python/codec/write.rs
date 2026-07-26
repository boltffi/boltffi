use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumId,
    MapKind, Op, Primitive, RecordId, ValueRef,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::{
            operation::Operation,
            read::EnumCodec,
            value::{SelfPositionAccess, ValueExpression},
        },
        cpython::render::primitive,
        render::Package,
        syntax::{CallExpression, Expression, Identifier, Literal},
    },
};

pub struct Writer<'package> {
    package: &'package Package<'package>,
    self_position_access: SelfPositionAccess,
}

impl<'package> Writer<'package> {
    pub fn new(package: &'package Package<'package>) -> Self {
        Self {
            package,
            self_position_access: SelfPositionAccess::Subscript,
        }
    }

    pub fn with_self_position_access(
        package: &'package Package<'package>,
        self_position_access: SelfPositionAccess,
    ) -> Self {
        Self {
            package,
            self_position_access,
        }
    }

    pub fn single(expressions: Vec<Result<Expression>>) -> Result<Expression> {
        let mut expressions = expressions.into_iter().collect::<Result<Vec<_>>>()?;
        match expressions.len() {
            1 => Ok(expressions.remove(0)),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "multi-statement wire writer",
            }),
        }
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::with_self_position_access(value, self.self_position_access).render()
    }

    fn binder(binder: BinderId) -> Result<Identifier> {
        ValueExpression::binder(binder)
    }

    fn call(
        name: Identifier,
        arguments: impl IntoIterator<Item = Expression>,
    ) -> Result<Expression> {
        Ok(Expression::call(arguments.into_iter().fold(
            CallExpression::new(Expression::identifier(name)),
            CallExpression::positional,
        )))
    }

    fn write_primitive(&self, value: Expression, primitive: Primitive) -> Result<Expression> {
        let stem = primitive::Runtime::new(primitive).wire_stem()?;
        Self::call(Identifier::parse(format!("_boltffi_wire_{stem}"))?, [value])
    }

    fn write_enum(&self, value: Expression, enumeration: EnumId) -> Result<Expression> {
        match self.package.enum_codec(enumeration)? {
            EnumCodec::CStyle(primitive) => {
                let stem = primitive::Runtime::new(primitive).wire_stem()?;
                let enum_name = self.package.enum_name(enumeration)?;
                let wire_value = Self::call(
                    Identifier::parse("_boltffi_enum_value")?,
                    [
                        value,
                        Expression::identifier(enum_name.clone()),
                        Expression::literal(Literal::string(enum_name.as_str())),
                    ],
                )?;
                Self::call(
                    Identifier::parse(format!("_boltffi_wire_{stem}"))?,
                    [wire_value],
                )
            }
            EnumCodec::Data { .. } => Ok(Expression::call(CallExpression::new(
                Expression::attribute(value, Identifier::parse("_boltffi_wire")?),
            ))),
        }
    }

    fn write_builtin(value: Expression, builtin: BuiltinType) -> Result<Expression> {
        match builtin {
            BuiltinType::Duration => {
                Self::call(Identifier::parse("_boltffi_wire_duration")?, [value])
            }
            BuiltinType::SystemTime => {
                Self::call(Identifier::parse("_boltffi_wire_system_time")?, [value])
            }
            BuiltinType::Uuid => Self::call(Identifier::parse("_boltffi_wire_uuid")?, [value]),
            BuiltinType::Url => Self::call(Identifier::parse("_boltffi_wire_url")?, [value]),
        }
    }
}

impl<'package> CodecWrite for Writer<'package> {
    type Stmt = Result<Expression>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| self.write_primitive(value, primitive)),
        ]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| Self::call(Identifier::parse("_boltffi_wire_string")?, [value])),
        ]
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Vec<Self::Stmt> {
        unreachable!(
            "InternedString codec write reached Python renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| Self::call(Identifier::parse("_boltffi_wire_bytes")?, [value])),
        ]
    }

    fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.encoded_record(id, value)
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            self.package.record_name(id).and_then(|_| {
                Ok(Expression::call(CallExpression::new(
                    Expression::attribute(value, Identifier::parse("_boltffi_wire")?),
                )))
            })
        })]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| self.write_enum(value, id)),
        ]
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            self.package.enum_codec(id).and_then(|_| {
                Ok(Expression::call(CallExpression::new(
                    Expression::attribute(value, Identifier::parse("_boltffi_wire")?),
                )))
            })
        })]
    }

    fn class_handle(&mut self, id: ClassId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|_| {
            self.package.class_name(&id)?;
            Err(Error::UnsupportedTarget {
                target: "python",
                shape: "class handle in wire writer",
            })
        })]
    }

    fn callback_handle(&mut self, _: CallbackId, _: &ValueRef) -> Vec<Self::Stmt> {
        vec![Err(Error::UnsupportedTarget {
            target: "python",
            shape: "callback handle in wire writer",
        })]
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
        match self.package.custom_type(id) {
            Ok(_) => {
                let representation = representation(self, value);
                vec![Self::single(representation)]
            }
            Err(error) => vec![Err(error)],
        }
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| Self::write_builtin(value, kind)),
        ]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Self::call(
                Identifier::parse("_boltffi_wire_optional")?,
                [
                    value,
                    Expression::lambda(Self::binder(binder)?, Self::single(inner)?),
                ],
            )
        })]
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        len: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|wire_value| {
            let count = len.render_with(&mut Operation::new(value, self.self_position_access));
            Self::call(
                Identifier::parse("_boltffi_wire_sequence")?,
                [
                    wire_value,
                    count?,
                    Expression::lambda(Self::binder(binder)?, Self::single(element)?),
                ],
            )
        })]
    }

    fn tuple(&mut self, value: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|_| {
            elements
                .into_iter()
                .map(Self::single)
                .collect::<Result<Vec<_>>>()
                .and_then(|fields| {
                    Ok(Expression::call(
                        CallExpression::new(Expression::attribute(
                            Expression::literal(Literal::bytes_empty()),
                            Identifier::parse("join")?,
                        ))
                        .positional(Expression::tuple(fields)),
                    ))
                })
        })]
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Self::call(
                Identifier::parse("_boltffi_wire_result")?,
                [
                    value,
                    Expression::lambda(Self::binder(binder)?, Self::single(ok)?),
                    Expression::lambda(Self::binder(binder)?, Self::single(err)?),
                ],
            )
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
        vec![self.value(value).and_then(|value| {
            let function = match kind {
                MapKind::Hash | MapKind::BTree => Identifier::parse("_boltffi_wire_map")?,
            };
            Self::call(
                function,
                [
                    value,
                    Expression::lambda(Self::binder(key_binder)?, Self::single(key)?),
                    Expression::lambda(Self::binder(value_binder)?, Self::single(map_value)?),
                ],
            )
        })]
    }
}
