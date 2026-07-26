use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecSize, CustomTypeId, ElementCount, EnumId,
    MapKind, Op, Primitive as BindingPrimitive, RecordId, ValueRef,
};

use crate::{
    core::{RenderContext, Result},
    target::java::{
        JavaHost, JavaVersion,
        codec::{
            SequenceElement,
            value::{ValueExpression, ValueMemberAccess},
        },
        primitive::Primitive,
        syntax::{ArgumentList, Expression, Identifier, TypeIdentifier, TypeName},
    },
};

pub struct Sizer<'context> {
    current: Expression,
    member_access: ValueMemberAccess,
    version: JavaVersion,
    context: &'context RenderContext<'context, boltffi_binding::Native>,
}

pub struct SizeExpression {
    expression: Expression,
    sequence_element: SequenceElement,
}

impl<'context> Sizer<'context> {
    pub fn new(
        version: JavaVersion,
        context: &'context RenderContext<'context, boltffi_binding::Native>,
    ) -> Self {
        Self {
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

    fn runtime_call(
        &self,
        method: &'static str,
        arguments: impl IntoIterator<Item = Expression>,
    ) -> Expression {
        Expression::static_call(
            TypeName::named(TypeIdentifier::known("WireSizes", self.version)),
            Identifier::known(method),
            arguments.into_iter().collect::<ArgumentList>(),
        )
    }

    fn unsupported(shape: &'static str) -> Result<SizeExpression> {
        Err(JavaHost::unsupported(shape))
    }
}

impl SizeExpression {
    pub fn into_expression(self) -> Expression {
        self.expression
    }

    fn new(expression: Expression) -> Self {
        Self {
            expression,
            sequence_element: SequenceElement::General,
        }
    }

    fn primitive(primitive: Primitive) -> Self {
        Self {
            expression: Expression::integer(primitive.wire_size()),
            sequence_element: SequenceElement::Primitive(primitive),
        }
    }

    fn string(expression: Expression) -> Self {
        Self {
            expression,
            sequence_element: SequenceElement::String,
        }
    }

    fn fixed(size: u64) -> Self {
        Self {
            expression: Expression::integer(size),
            sequence_element: SequenceElement::Fixed(size),
        }
    }
}

impl CodecSize for Sizer<'_> {
    type Expr = Result<SizeExpression>;

    fn primitive(&mut self, primitive: BindingPrimitive, _value: &ValueRef) -> Self::Expr {
        Primitive::try_from(primitive).map(SizeExpression::primitive)
    }

    fn string(&mut self, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::string(
            self.runtime_call("string", [self.value(value)?]),
        ))
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Self::Expr {
        // Java does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec size reached Java renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(Expression::integer(4).add(
            self.value(value)?.member(Identifier::known("length")),
        )))
    }

    fn direct_record(&mut self, _id: RecordId, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(self.value(value)?.call(
            Identifier::known("wireSize"),
            ArgumentList::default(),
        )))
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Self::Expr {
        self.direct_record(id, value)
    }

    fn c_style_enum(&mut self, id: EnumId, _value: &ValueRef) -> Self::Expr {
        crate::target::java::render::Enumeration::c_style_primitive(id, self.context)
            .map(Primitive::wire_size)
            .map(SizeExpression::fixed)
    }

    fn data_enum(&mut self, _id: EnumId, value: &ValueRef) -> Self::Expr {
        Ok(SizeExpression::new(self.value(value)?.call(
            Identifier::known("wireSize"),
            ArgumentList::default(),
        )))
    }

    fn class_handle(&mut self, _id: ClassId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("class handle wire size")
    }

    fn callback_handle(&mut self, _id: CallbackId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("callback handle wire size")
    }

    fn custom<F>(&mut self, _id: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
    where
        F: FnOnce(&mut Self, &ValueRef) -> Self::Expr,
    {
        representation(self, value)
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
        match kind {
            BuiltinType::Duration | BuiltinType::SystemTime => {
                Ok(SizeExpression::new(Expression::integer(12)))
            }
            BuiltinType::Uuid => Ok(SizeExpression::new(Expression::integer(16))),
            BuiltinType::Url => Ok(SizeExpression::new(
                self.runtime_call(
                    "string",
                    [self
                        .value(value)?
                        .call(Identifier::known("toString"), ArgumentList::default())],
                ),
            )),
        }
    }

    fn optional(&mut self, value: &ValueRef, binder: BinderId, inner: Self::Expr) -> Self::Expr {
        Ok(SizeExpression::new(self.runtime_call(
            "optional",
            [
                self.value(value)?,
                Expression::lambda(
                    [ValueExpression::binder(binder, self.version)?],
                    inner?.expression,
                ),
            ],
        )))
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _len: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let element = element?;
        match element.sequence_element {
            SequenceElement::Primitive(primitive) => Ok(SizeExpression::new(
                Expression::integer(4).add(
                    value
                        .member(Identifier::known("length"))
                        .multiply(Expression::integer(primitive.wire_size())),
                ),
            )),
            SequenceElement::String => Ok(SizeExpression::new(
                self.runtime_call("stringSequence", [value]),
            )),
            SequenceElement::Fixed(size) => Ok(SizeExpression::new(
                Expression::integer(4).add(
                    value
                        .call(Identifier::known("size"), ArgumentList::default())
                        .multiply(Expression::integer(size)),
                ),
            )),
            SequenceElement::General => Ok(SizeExpression::new(self.runtime_call(
                "sequence",
                [
                    value,
                    Expression::lambda(
                        [ValueExpression::binder(binder, self.version)?],
                        element.expression,
                    ),
                ],
            ))),
        }
    }

    fn tuple(&mut self, _value: &ValueRef, _elements: Vec<Self::Expr>) -> Self::Expr {
        Self::unsupported("tuple wire size")
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr {
        let binder = ValueExpression::binder(binder, self.version)?;
        Ok(SizeExpression::new(
            self.value(value)?.call(
                Identifier::known("wireSize"),
                [
                    Expression::lambda([binder.clone()], ok?.expression),
                    Expression::lambda([binder], err?.expression),
                ]
                .into_iter()
                .collect(),
            ),
        ))
    }

    fn map(
        &mut self,
        _kind: MapKind,
        value: &ValueRef,
        key_binder: BinderId,
        key: Self::Expr,
        value_binder: BinderId,
        map_value: Self::Expr,
    ) -> Self::Expr {
        Ok(SizeExpression::new(self.runtime_call(
            "map",
            [
                self.value(value)?,
                Expression::lambda(
                    [ValueExpression::binder(key_binder, self.version)?],
                    key?.expression,
                ),
                Expression::lambda(
                    [ValueExpression::binder(value_binder, self.version)?],
                    map_value?.expression,
                ),
            ],
        )))
    }
}
