use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, MapKind,
    Native, Op, Primitive as BindingPrimitive, RecordId,
};

use crate::{
    core::{RenderContext, Result},
    target::java::{
        JavaHost, JavaPackage, JavaVersion,
        codec::SequenceElement,
        primitive::Primitive,
        render::{Enumeration, Record, ResultClass},
        syntax::{ArgumentList, Expression, Identifier, TypeName},
    },
};

pub struct Reader<'context> {
    reader: Identifier,
    version: JavaVersion,
    context: &'context RenderContext<'context, Native>,
    package: Option<&'context JavaPackage>,
}

pub struct ReadExpression {
    expression: Expression,
    sequence_element: SequenceElement,
}

impl<'context> Reader<'context> {
    pub fn new(
        reader: Identifier,
        version: JavaVersion,
        context: &'context RenderContext<'context, Native>,
    ) -> Self {
        Self {
            reader,
            version,
            context,
            package: None,
        }
    }

    pub fn package(mut self, package: &'context JavaPackage) -> Self {
        self.package = Some(package);
        self
    }

    fn call(&self, method: impl Into<String>) -> Result<ReadExpression> {
        Ok(ReadExpression::new(
            Expression::identifier(self.reader.clone()).call(
                Identifier::parse_for(method, self.version)?,
                ArgumentList::default(),
            ),
        ))
    }

    fn record(&self, id: RecordId) -> Result<ReadExpression> {
        Ok(ReadExpression::new(Expression::static_call(
            self.generated_type(Record::type_name_for(id, self.context, self.version)?),
            Identifier::known("fromReader"),
            [Expression::identifier(self.reader.clone())]
                .into_iter()
                .collect(),
        )))
    }

    fn unsupported(shape: &'static str) -> Result<ReadExpression> {
        Err(JavaHost::unsupported(shape))
    }

    fn generated_type(&self, name: crate::target::java::syntax::TypeIdentifier) -> TypeName {
        match self.package {
            Some(package) => package.type_name(name),
            None => TypeName::named(name),
        }
    }
}

impl ReadExpression {
    pub fn into_expression(self) -> Expression {
        self.expression
    }

    fn new(expression: Expression) -> Self {
        Self {
            expression,
            sequence_element: SequenceElement::General,
        }
    }

    fn primitive(primitive: Primitive, expression: Expression) -> Self {
        Self {
            expression,
            sequence_element: SequenceElement::Primitive(primitive),
        }
    }

    fn string(expression: Expression) -> Self {
        Self {
            expression,
            sequence_element: SequenceElement::String,
        }
    }
}

impl CodecRead for Reader<'_> {
    type Expr = Result<ReadExpression>;

    fn primitive(&mut self, primitive: BindingPrimitive) -> Self::Expr {
        let primitive = Primitive::try_from(primitive)?;
        self.call(format!("read{}", primitive.wire_method_suffix()))
            .map(|expression| ReadExpression::primitive(primitive, expression.expression))
    }

    fn string(&mut self) -> Self::Expr {
        self.call("readString")
            .map(|expression| ReadExpression::string(expression.expression))
    }

    fn interned_string(&mut self, _static_values: &[String]) -> Self::Expr {
        // Java does not advertise InternedString capability; the capability gate
        // ensures this branch is never reached for valid bindings.
        unreachable!(
            "InternedString codec read reached Java renderer: host does not advertise InternedString capability"
        )
    }

    fn bytes(&mut self) -> Self::Expr {
        self.call("readBytes")
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        self.record(id)
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        self.record(id)
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        let primitive = Enumeration::c_style_primitive(id, self.context)?;
        let enumeration = Enumeration::type_name_for(id, self.context, self.version)?;
        self.call(format!("read{}", primitive.wire_method_suffix()))
            .map(|value| {
                ReadExpression::new(Expression::static_call(
                    self.generated_type(enumeration),
                    Identifier::known("fromValue"),
                    [value.expression].into_iter().collect(),
                ))
            })
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        Enumeration::type_name_for(id, self.context, self.version).map(|enumeration| {
            ReadExpression::new(Expression::static_call(
                self.generated_type(enumeration),
                Identifier::known("fromReader"),
                [Expression::identifier(self.reader.clone())]
                    .into_iter()
                    .collect(),
            ))
        })
    }

    fn class_handle(&mut self, _id: ClassId) -> Self::Expr {
        Self::unsupported("class handle wire read")
    }

    fn callback_handle(&mut self, _id: CallbackId) -> Self::Expr {
        Self::unsupported("callback handle wire read")
    }

    fn custom(&mut self, _id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        representation
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        self.call(match kind {
            BuiltinType::Duration => "readDuration",
            BuiltinType::SystemTime => "readInstant",
            BuiltinType::Uuid => "readUuid",
            BuiltinType::Url => "readUri",
        })
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(
            Expression::identifier(self.reader.clone()).call(
                Identifier::known("readOptional"),
                [Expression::lambda([], inner?.expression)]
                    .into_iter()
                    .collect(),
            ),
        ))
    }

    fn sequence(&mut self, _len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        let element = element?;
        match element.sequence_element {
            SequenceElement::Primitive(primitive) => {
                self.call(format!("read{}Array", primitive.wire_method_suffix()))
            }
            SequenceElement::String => self.call("readStringSequence"),
            SequenceElement::General | SequenceElement::Fixed(_) => Ok(ReadExpression::new(
                Expression::identifier(self.reader.clone()).call(
                    Identifier::known("readSequence"),
                    [Expression::lambda([], element.expression)]
                        .into_iter()
                        .collect(),
                ),
            )),
        }
    }

    fn tuple(&mut self, _elements: Vec<Self::Expr>) -> Self::Expr {
        Self::unsupported("tuple wire read")
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::static_call(
            TypeName::named(ResultClass::type_name(self.version)),
            Identifier::known("read"),
            [
                Expression::identifier(self.reader.clone()),
                Expression::lambda([], ok?.expression),
                Expression::lambda([], err?.expression),
            ]
            .into_iter()
            .collect(),
        )))
    }

    fn map(&mut self, _kind: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(
            Expression::identifier(self.reader.clone()).call(
                Identifier::known("readMap"),
                [
                    Expression::lambda([], key?.expression),
                    Expression::lambda([], value?.expression),
                ]
                .into_iter()
                .collect(),
            ),
        ))
    }
}
