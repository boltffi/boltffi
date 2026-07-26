use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecSize, CustomTypeId, ElementCount, EnumId,
    MapKind, Native, Op, Primitive, RecordId, ValueRef,
};

use crate::{
    core::{RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::value::ValueExpression,
        primitive::KotlinPrimitive,
        render::Enumeration,
        syntax::{ArgumentList, Expression, Identifier},
        tuple::Arity,
    },
};

pub struct Sizer<'context> {
    current: Expression,
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
}

pub struct SizeExpression {
    expression: Expression,
    primitive: Option<Primitive>,
}

impl<'context> Sizer<'context> {
    pub fn new(
        host: &'context KotlinHost,
        context: &'context RenderContext<'context, Native>,
    ) -> Result<Self> {
        Ok(Self {
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

    fn fixed(bytes: u64) -> Result<SizeExpression> {
        Ok(SizeExpression::new(Expression::integer(bytes)))
    }

    fn string_size(&self, value: &ValueRef) -> Result<SizeExpression> {
        self.string_expression_size(self.value(value)?)
    }

    fn string_expression_size(&self, value: Expression) -> Result<SizeExpression> {
        Ok(SizeExpression::new(Expression::integer(4).add(
            Expression::call(
                "Utf8Codec",
                Identifier::parse("maxBytes")?,
                [value].into_iter().collect::<ArgumentList>(),
            ),
        )))
    }

    fn bytes_size(&self, value: &ValueRef) -> Result<SizeExpression> {
        Ok(SizeExpression::new(Expression::integer(4).add(
            Expression::property(self.value(value)?, Identifier::parse("size")?),
        )))
    }

    fn encoded_record_size(&self, value: &ValueRef) -> Result<SizeExpression> {
        Ok(SizeExpression::new(Expression::call(
            self.value(value)?,
            Identifier::parse("wireSize")?,
            ArgumentList::default(),
        )))
    }

    fn primitive_size(primitive: Primitive) -> Result<SizeExpression> {
        KotlinPrimitive::new(primitive)
            .wire_size()
            .map(Expression::integer)
            .map(|expression| SizeExpression::primitive(primitive, expression))
    }

    fn enum_size(&self, id: EnumId) -> Result<SizeExpression> {
        Enumeration::from_id(id, self.host, self.context).and_then(|enumeration| {
            KotlinPrimitive::new(enumeration.repr()?)
                .wire_size()
                .map(Expression::integer)
                .map(SizeExpression::new)
        })
    }

    fn unsupported(shape: &'static str) -> Result<SizeExpression> {
        Err(KotlinHost::unsupported(shape))
    }

    fn with_current(
        &mut self,
        current: Expression,
        render: impl FnOnce(&mut Self, &ValueRef) -> Result<SizeExpression>,
    ) -> Result<SizeExpression> {
        let previous = std::mem::replace(&mut self.current, current);
        let expression = render(self, &ValueRef::self_value());
        self.current = previous;
        expression
    }
}

impl SizeExpression {
    pub fn into_expression(self) -> Expression {
        self.expression
    }

    fn new(expression: Expression) -> Self {
        Self {
            expression,
            primitive: None,
        }
    }

    fn primitive(primitive: Primitive, expression: Expression) -> Self {
        Self {
            expression,
            primitive: Some(primitive),
        }
    }

    fn without_primitive(mut self) -> Self {
        self.primitive = None;
        self
    }
}

impl CodecSize for Sizer<'_> {
    type Expr = Result<SizeExpression>;

    fn primitive(&mut self, primitive: Primitive, _value: &ValueRef) -> Self::Expr {
        Self::primitive_size(primitive)
    }

    fn string(&mut self, value: &ValueRef) -> Self::Expr {
        self.string_size(value)
    }

    fn interned_string(&mut self, _static_values: &[String], _value: &ValueRef) -> Self::Expr {
        // Kotlin does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec size reached Kotlin renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
        self.bytes_size(value)
    }

    fn direct_record(&mut self, _id: RecordId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("direct-record wire size")
    }

    fn encoded_record(&mut self, _id: RecordId, value: &ValueRef) -> Self::Expr {
        self.encoded_record_size(value)
    }

    fn c_style_enum(&mut self, id: EnumId, _value: &ValueRef) -> Self::Expr {
        self.enum_size(id)
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr {
        Enumeration::size_expression(id, self.value(value)?, self.context).map(SizeExpression::new)
    }

    fn class_handle(&mut self, _id: ClassId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("class handle wire size")
    }

    fn callback_handle(&mut self, _id: CallbackId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("callback handle wire size")
    }

    fn custom<F>(&mut self, id: CustomTypeId, value: &ValueRef, representation: F) -> Self::Expr
    where
        F: FnOnce(&mut Self, &ValueRef) -> Self::Expr,
    {
        let representation = match self.context.custom_type_mapping(id) {
            Some(mapping) => self
                .value(value)
                .and_then(|value| KotlinHost::custom_type_encode(mapping, value))
                .and_then(|value| self.with_current(value, representation)),
            None => representation(self, value),
        };
        representation.map(SizeExpression::without_primitive)
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
        match kind {
            BuiltinType::Duration | BuiltinType::SystemTime => Self::fixed(12),
            BuiltinType::Uuid => Self::fixed(16),
            BuiltinType::Url => self
                .string_expression_size(self.value(value)?.convert(Identifier::parse("toString")?)),
        }
    }

    fn optional(&mut self, value: &ValueRef, binder: BinderId, inner: Self::Expr) -> Self::Expr {
        let value = self.value(value)?;
        let binder = ValueExpression::binder(binder)?;
        Ok(SizeExpression::new(
            value.optional_size(binder, inner?.expression),
        ))
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _len: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let binder = ValueExpression::binder(binder)?;
        let element = element?;
        match element.primitive {
            Some(primitive) => {
                let size = Identifier::parse("size")?;
                KotlinPrimitive::new(primitive).wire_size().map(|width| {
                    SizeExpression::new(Expression::integer(4).add(
                        Expression::property(value, size).multiply(Expression::integer(width)),
                    ))
                })
            }
            None => Ok(SizeExpression::new(
                Expression::integer(4).add(value.sum_of(binder, element.expression)),
            )),
        }
    }

    fn tuple(&mut self, _value: &ValueRef, elements: Vec<Self::Expr>) -> Self::Expr {
        Arity::from_count(elements.len())?;
        elements
            .into_iter()
            .try_fold(Expression::integer(0), |total, element| {
                element.map(|element| total.add(element.expression))
            })
            .map(SizeExpression::new)
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let binder = ValueExpression::binder(binder)?;
        Ok(SizeExpression::new(value.result_size(
            binder,
            ok?.expression,
            err?.expression,
        )))
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
        let value = self.value(value)?;
        let key_binder = ValueExpression::binder(key_binder)?;
        let value_binder = ValueExpression::binder(value_binder)?;
        Ok(SizeExpression::new(value.map_size(
            key_binder,
            key?.expression,
            value_binder,
            map_value?.expression,
        )))
    }
}
