use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, MapKind,
    Native, Op, Primitive, RecordId,
};

use crate::{
    core::{RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        name_style::KotlinPackage,
        primitive::KotlinPrimitive,
        render::{Enumeration, Record},
        syntax::{ArgumentList, Expression, Identifier, TypeName},
        tuple::Arity,
    },
};

pub struct Reader<'context> {
    reader: Identifier,
    host: &'context KotlinHost,
    context: &'context RenderContext<'context, Native>,
    package: Option<KotlinPackage>,
}

pub struct ReadExpression {
    expression: Expression,
    primitive: Option<Primitive>,
}

impl<'context> Reader<'context> {
    pub fn new(
        reader: Identifier,
        host: &'context KotlinHost,
        context: &'context RenderContext<'context, Native>,
    ) -> Self {
        Self {
            reader,
            host,
            context,
            package: None,
        }
    }

    pub fn package(mut self, package: &KotlinPackage) -> Self {
        self.package = Some(package.clone());
        self
    }

    fn call(&self, method: impl Into<String>) -> Result<ReadExpression> {
        Ok(ReadExpression::new(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse(method)?,
            ArgumentList::default(),
        )))
    }

    fn unsupported(shape: &'static str) -> Result<ReadExpression> {
        Err(KotlinHost::unsupported(shape))
    }

    fn qualified_type(&self, ty: TypeName) -> TypeName {
        self.package
            .as_ref()
            .map_or(ty.clone(), |package| TypeName::qualified(package, ty))
    }

    fn enum_type(&self, id: EnumId) -> Result<TypeName> {
        Enumeration::type_name_from_id(id, self.context).map(|ty| self.qualified_type(ty))
    }
}

impl ReadExpression {
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
}

impl CodecRead for Reader<'_> {
    type Expr = Result<ReadExpression>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        KotlinPrimitive::new(primitive)
            .wire_method_suffix()
            .and_then(|suffix| self.call(format!("read{suffix}")))
            .map(|expression| ReadExpression::primitive(primitive, expression.expression))
    }

    fn string(&mut self) -> Self::Expr {
        self.call("readString")
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        // Kotlin does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec read reached Kotlin renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Expr {
        self.call("readBytes")
    }

    fn direct_record(&mut self, _id: RecordId) -> Self::Expr {
        Self::unsupported("direct-record wire read")
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        Record::type_name_from_id(id, self.context).and_then(|record| {
            let record = self.qualified_type(record);
            Ok(ReadExpression::new(Expression::call(
                record,
                Identifier::parse("fromReader")?,
                [Expression::identifier(self.reader.clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )))
        })
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        Enumeration::from_id(id, self.host, self.context).and_then(|enumeration| {
            KotlinPrimitive::new(enumeration.repr()?)
                .native_wire_method_suffix()
                .and_then(|suffix| {
                    let ty = self.enum_type(id)?;
                    self.call(format!("read{suffix}")).and_then(|value| {
                        Ok(ReadExpression::new(Expression::call(
                            ty,
                            Identifier::parse("fromValue")?,
                            [value.expression].into_iter().collect::<ArgumentList>(),
                        )))
                    })
                })
        })
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        self.enum_type(id).and_then(|enumeration| {
            Ok(ReadExpression::new(Expression::call(
                enumeration,
                Identifier::parse("fromReader")?,
                [Expression::identifier(self.reader.clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )))
        })
    }

    fn class_handle(&mut self, _id: ClassId) -> Self::Expr {
        Self::unsupported("class handle wire read")
    }

    fn callback_handle(&mut self, _id: CallbackId) -> Self::Expr {
        Self::unsupported("callback handle wire read")
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        let representation = representation?;
        match self.context.custom_type_mapping(id) {
            Some(mapping) => KotlinHost::custom_type_decode(mapping, representation.expression)
                .map(ReadExpression::new),
            None => Ok(ReadExpression::new(representation.expression)),
        }
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        let method = match kind {
            BuiltinType::Duration => "readDuration",
            BuiltinType::SystemTime => "readInstant",
            BuiltinType::Uuid => "readUuid",
            BuiltinType::Url => "readUri",
        };
        self.call(method)
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse("readOptionalValue")?,
            [Expression::lambda_expression(
                vec![self.reader.clone()],
                inner?.expression,
            )]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }

    fn sequence(&mut self, _len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        match element.primitive {
            Some(primitive) => KotlinPrimitive::new(primitive)
                .wire_array_method_suffix()
                .and_then(|suffix| self.call(format!("read{suffix}Array"))),
            None => Ok(ReadExpression::new(Expression::call(
                Expression::identifier(self.reader.clone()),
                Identifier::parse("readSequence")?,
                [Expression::lambda_expression(
                    vec![self.reader.clone()],
                    element.expression,
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            ))),
        }
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        let arity = Arity::from_count(elements.len())?;
        elements
            .into_iter()
            .map(|element| element.map(ReadExpression::into_expression))
            .collect::<Result<Vec<_>>>()
            .and_then(|elements| arity.expression(elements))
            .map(ReadExpression::new)
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse("readResult")?,
            [
                Expression::lambda_expression(vec![self.reader.clone()], ok?.expression),
                Expression::lambda_expression(vec![self.reader.clone()], err?.expression),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }

    fn map(&mut self, _kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse("readMap")?,
            [
                Expression::lambda_expression(vec![self.reader.clone()], key?.expression),
                Expression::lambda_expression(vec![self.reader.clone()], value?.expression),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }
}
