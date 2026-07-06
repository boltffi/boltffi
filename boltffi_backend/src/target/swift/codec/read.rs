use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumDecl, EnumId,
    MapKind, Native, Op, Primitive, RecordId,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::swift::{
        SwiftHost,
        primitive::SwiftPrimitive,
        render::SwiftType,
        syntax::{ArgumentList, Expression, Identifier},
    },
};

pub struct Reader<'context, 'bindings> {
    name: Identifier,
    context: &'context RenderContext<'bindings, Native>,
}

pub struct ReadExpression {
    expression: Expression,
    value: ReadValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadValue {
    String,
    Other,
}

impl<'context, 'bindings> Reader<'context, 'bindings> {
    pub fn new(name: Identifier, context: &'context RenderContext<'bindings, Native>) -> Self {
        Self { name, context }
    }

    fn unsupported<T>(&self, shape: &'static str) -> Result<T> {
        Err(SwiftHost::unsupported(shape))
    }

    fn read(&self, method: &str) -> ReadExpression {
        ReadExpression::new(Expression::call(
            Expression::member(&self.name, method),
            ArgumentList::default(),
        ))
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
                invariant: "missing enum type in Swift codec reader",
            }),
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
            value: ReadValue::Other,
        }
    }

    fn string(expression: Expression) -> Self {
        Self {
            expression,
            value: ReadValue::String,
        }
    }

    fn result_error(self) -> Expression {
        match self.value {
            ReadValue::String => Expression::call(
                "FfiError",
                [Expression::labeled("message", self.expression)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
            ReadValue::Other => self.expression,
        }
    }
}

impl CodecRead for Reader<'_, '_> {
    type Expr = Result<ReadExpression>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        SwiftPrimitive::new(primitive)
            .read_expression(self.name.clone())
            .map(ReadExpression::new)
    }

    fn string(&mut self) -> Self::Expr {
        Ok(ReadExpression::string(self.read("readString").expression))
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok(self.read("readBytes"))
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::member(SwiftType::record(id, self.context)?, "decode"),
            [Expression::labeled("from", Expression::address(&self.name))]
                .into_iter()
                .collect::<ArgumentList>(),
        )))
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::member(SwiftType::record(id, self.context)?, "decode"),
            [Expression::labeled("from", Expression::address(&self.name))]
                .into_iter()
                .collect::<ArgumentList>(),
        )))
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        Ok(ReadExpression::new(Expression::forced(Expression::call(
            SwiftType::enumeration(id, self.context)?,
            [Expression::labeled(
                "rawValue",
                SwiftPrimitive::new(self.c_style_enum_repr(id)?)
                    .read_expression(self.name.clone())?,
            )]
            .into_iter()
            .collect::<ArgumentList>(),
        ))))
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::member(SwiftType::enumeration(id, self.context)?, "decode"),
            [Expression::labeled("from", Expression::address(&self.name))]
                .into_iter()
                .collect::<ArgumentList>(),
        )))
    }

    fn class_handle(&mut self, _: ClassId) -> Self::Expr {
        self.unsupported("class handle codec read")
    }

    fn callback_handle(&mut self, _: CallbackId) -> Self::Expr {
        self.unsupported("callback handle codec read")
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        let representation = representation?;
        match self.context.custom_type_mapping(id) {
            Some(mapping) => SwiftHost::custom_type_decode(mapping, representation.expression)
                .map(ReadExpression::new),
            None => Ok(ReadExpression::new(representation.expression)),
        }
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        Ok(self.read(match kind {
            BuiltinType::Duration => "readDuration",
            BuiltinType::SystemTime => "readTimestamp",
            BuiltinType::Uuid => "readUuid",
            BuiltinType::Url => "readUrl",
        }))
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::trailing_closure(
            Expression::member(&self.name, "readOptional"),
            ArgumentList::default(),
            self.name.clone(),
            inner?.expression,
        )))
    }

    fn sequence(&mut self, _: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::trailing_closure(
            Expression::member(&self.name, "readArray"),
            ArgumentList::default(),
            self.name.clone(),
            element?.expression,
        )))
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        elements
            .into_iter()
            .map(|element| element.map(ReadExpression::into_expression))
            .collect::<Result<Vec<_>>>()
            .map(Expression::tuple)
            .map(ReadExpression::new)
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::member(&self.name, "readResult"),
            [
                Expression::closure([self.name.clone()], ok?.expression),
                Expression::closure([self.name.clone()], err?.result_error()),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }

    fn map(&mut self, _: MapKind, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        Ok(ReadExpression::new(Expression::call(
            Expression::member(&self.name, "readMap"),
            [
                Expression::closure([self.name.clone()], key?.expression),
                Expression::closure([self.name.clone()], value?.expression),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        )))
    }
}
