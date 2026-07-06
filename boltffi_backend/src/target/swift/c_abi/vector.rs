use boltffi_binding::{
    DirectVectorElementType, DirectVectorPrimitive, Native, Primitive, RecordId,
};

use std::iter;

use crate::{
    bridge::c::CBridgeContract,
    core::{Error, RenderContext, Result},
    target::swift::{
        SwiftHost,
        name_style::{GeneratedLocal, Name},
        primitive::SwiftPrimitive,
        render::SwiftType,
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectVector {
    ty: TypeName,
    storage: Storage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowedVector {
    setup: Vec<Statement>,
    collection: Expression,
    buffer: Identifier,
    scope: Scope,
    arguments: Vec<Expression>,
    copy_pointer: Expression,
    copy_length: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReturnedVector {
    ty: TypeName,
    element: TypeName,
    decode: Decode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedVector {
    setup: Vec<Statement>,
    value: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CopiedVector {
    result: Identifier,
    borrowed: BorrowedVector,
    copy: Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Storage {
    Primitive { element: TypeName },
    Record { swift: TypeName, storage: TypeName },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Decode {
    Primitive,
    Record { swift: TypeName },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    TypedBuffer,
    RawBytes,
}

impl DirectVector {
    pub fn from_element(
        element: &DirectVectorElementType,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Self::from_primitive(*primitive),
            DirectVectorElementType::Record(record) => Self::from_record(*record, bridge, context),
            _ => Err(SwiftHost::unsupported("unknown direct-vector type")),
        }
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn borrowed(&self, source_name: &Name, value: Identifier) -> Result<BorrowedVector> {
        self.borrowed_with(
            value,
            source_name.generated("buffer")?,
            source_name.generated("storage")?,
        )
    }

    fn borrowed_with(
        &self,
        value: Identifier,
        buffer: Identifier,
        storage: Identifier,
    ) -> Result<BorrowedVector> {
        match &self.storage {
            Storage::Primitive { element } => Ok(BorrowedVector {
                setup: Vec::new(),
                collection: Expression::identifier(value),
                buffer,
                scope: Scope::TypedBuffer,
                arguments: Vec::new(),
                copy_pointer: Expression::nil(),
                copy_length: Expression::nil(),
            }
            .with_typed_arguments(element)),
            Storage::Record {
                storage: element, ..
            } => {
                let item = Identifier::parse("item")?;
                Ok(BorrowedVector {
                    setup: vec![Statement::new(
                        [
                            format!("var {storage}: {} = []", TypeName::array(element.clone())),
                            format!("{storage}.reserveCapacity({value}.count)"),
                            format!("for {item} in {value} {{"),
                            format!("    {storage}.append({}.cValue)", item),
                            "}".to_owned(),
                        ]
                        .join("\n"),
                    )],
                    collection: Expression::identifier(storage),
                    buffer,
                    scope: Scope::RawBytes,
                    arguments: Vec::new(),
                    copy_pointer: Expression::nil(),
                    copy_length: Expression::nil(),
                }
                .with_byte_arguments())
            }
        }
    }

    pub fn received(
        &self,
        source_name: &Name,
        pointer: Identifier,
        length: Identifier,
    ) -> Result<ReceivedVector> {
        self.received_with(
            pointer,
            length,
            source_name.generated("count")?,
            source_name.generated("value")?,
            source_name.generated("raw")?,
        )
    }

    pub fn received_with(
        &self,
        pointer: Identifier,
        length: Identifier,
        count: Identifier,
        value: Identifier,
        raw: Identifier,
    ) -> Result<ReceivedVector> {
        match &self.storage {
            Storage::Primitive { .. } => Ok(ReceivedVector {
                setup: vec![
                    Statement::let_value(
                        &count,
                        Expression::call(
                            TypeName::int(),
                            [Expression::identifier(length)]
                                .into_iter()
                                .collect::<ArgumentList>(),
                        ),
                    ),
                    Statement::let_value(
                        &value,
                        Expression::conditional(
                            Expression::equal(&count, "0"),
                            "[]",
                            Expression::call(
                                "Array",
                                [Expression::call(
                                    "UnsafeBufferPointer",
                                    [
                                        Expression::labeled("start", Expression::forced(pointer)),
                                        Expression::labeled(
                                            "count",
                                            Expression::identifier(count.clone()),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                                )]
                                .into_iter()
                                .collect::<ArgumentList>(),
                            ),
                        ),
                    ),
                ],
                value: Expression::identifier(value),
            }),
            Storage::Record { swift, storage } => {
                let index = Identifier::parse("index")?;
                Ok(ReceivedVector {
                    setup: vec![
                        Statement::let_value(
                            &count,
                            Expression::new(format!(
                                "Int({length}) / MemoryLayout<{storage}>.stride"
                            )),
                        ),
                        Statement::new(
                            [
                                format!("var {value}: {} = []", TypeName::array(swift.clone())),
                                format!("{value}.reserveCapacity({count})"),
                                format!("if {count} > 0 {{"),
                                format!(
                                    "    let {raw} = {}",
                                    Expression::call(
                                        Expression::member(
                                            Expression::call(
                                                "UnsafeRawPointer",
                                                [Expression::forced(pointer)]
                                                    .into_iter()
                                                    .collect::<ArgumentList>(),
                                            ),
                                            "assumingMemoryBound",
                                        ),
                                        [Expression::labeled("to", storage.clone().metatype())]
                                            .into_iter()
                                            .collect::<ArgumentList>(),
                                    )
                                ),
                                format!("for {index} in 0..<{count} {{"),
                                format!(
                                    "        {value}.append({swift}(fromC: {}))",
                                    Expression::subscript(&raw, &index)
                                ),
                                "}".to_owned(),
                                "}".to_owned(),
                            ]
                            .join("\n"),
                        ),
                    ],
                    value: Expression::identifier(value),
                })
            }
        }
    }

    pub fn pointer_ty(&self) -> TypeName {
        match &self.storage {
            Storage::Primitive { element } => element.clone().pointer(),
            Storage::Record { .. } => TypeName::uint8().pointer(),
        }
    }

    pub fn copied(&self, value: Identifier, copy: Identifier) -> Result<CopiedVector> {
        Ok(CopiedVector {
            result: GeneratedLocal::ReturnBuffer.identifier()?,
            borrowed: self.borrowed_with(
                value,
                GeneratedLocal::ReturnBuffer.suffixed("buffer")?,
                GeneratedLocal::ReturnBuffer.suffixed("storage")?,
            )?,
            copy,
        })
    }

    pub fn returned(&self) -> ReturnedVector {
        match &self.storage {
            Storage::Primitive { element } => ReturnedVector {
                ty: self.ty.clone(),
                element: element.clone(),
                decode: Decode::Primitive,
            },
            Storage::Record { swift, storage } => ReturnedVector {
                ty: self.ty.clone(),
                element: storage.clone(),
                decode: Decode::Record {
                    swift: swift.clone(),
                },
            },
        }
    }

    fn from_primitive(primitive: DirectVectorPrimitive) -> Result<Self> {
        let element = Self::primitive_type(primitive.primitive())?;
        Ok(Self {
            ty: TypeName::array(element.clone()),
            storage: Storage::Primitive { element },
        })
    }

    fn from_record(
        record: RecordId,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let swift = SwiftType::record(record, context)?;
        let storage = bridge
            .source_direct_record(record)
            .map(|record| TypeName::new(record.name()))
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing direct record C type for Swift direct vector",
            })?;
        Ok(Self {
            ty: TypeName::array(swift.clone()),
            storage: Storage::Record { swift, storage },
        })
    }

    fn primitive_type(primitive: Primitive) -> Result<TypeName> {
        SwiftPrimitive::new(primitive).api_type()
    }
}

impl BorrowedVector {
    pub fn arguments(&self) -> Vec<Expression> {
        self.arguments.clone()
    }

    pub fn wrap(&self, body: String, indent: &str, returns_value: bool) -> String {
        self.wrap_with_effect(body, indent, returns_value, false)
    }

    pub fn wrap_result(
        &self,
        body: String,
        indent: &str,
        returns_value: bool,
        throwing: bool,
    ) -> String {
        self.wrap_with_effect(body, indent, returns_value, throwing)
    }

    pub fn wrap_binding(
        &self,
        binding: &Identifier,
        body: String,
        indent: &str,
        throwing: bool,
    ) -> String {
        self.setup
            .iter()
            .map(|statement| statement.indented(indent))
            .chain(iter::once(Statement::binding_trailing_closure_scope(
                self.scope_call(),
                &self.buffer,
                binding,
                body,
                indent,
                throwing,
            )))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn wrap_with_effect(
        &self,
        body: String,
        indent: &str,
        returns_value: bool,
        throwing: bool,
    ) -> String {
        let scope = match returns_value {
            true => Statement::returning_trailing_closure_scope(
                self.scope_call(),
                &self.buffer,
                body,
                indent,
                throwing,
            ),
            false => Statement::discarding_trailing_closure_scope(
                self.scope_call(),
                &self.buffer,
                body,
                indent,
            ),
        };
        self.setup
            .iter()
            .map(|statement| statement.indented(indent))
            .chain(iter::once(scope))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn copy(&self, copy: &Identifier) -> Expression {
        Expression::call(
            copy,
            [self.copy_pointer.clone(), self.copy_length.clone()]
                .into_iter()
                .collect::<ArgumentList>(),
        )
    }

    fn with_typed_arguments(mut self, element: &TypeName) -> Self {
        self.arguments = vec![
            Expression::member(&self.buffer, "baseAddress"),
            self.count_argument(Expression::member(&self.buffer, "count")),
        ];
        self.copy_pointer = Expression::new(format!(
            "UnsafeRawPointer({}.baseAddress)?.assumingMemoryBound(to: UInt8.self)",
            self.buffer
        ));
        self.copy_length = self.count_argument(Expression::new(format!(
            "{}.count * MemoryLayout<{element}>.stride",
            self.buffer
        )));
        self
    }

    fn with_byte_arguments(mut self) -> Self {
        let pointer = Expression::call(
            Expression::member(
                Expression::optional_member(&self.buffer, "baseAddress"),
                "assumingMemoryBound",
            ),
            [Expression::labeled("to", TypeName::uint8().metatype())]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        self.arguments = vec![
            pointer.clone(),
            self.count_argument(Expression::member(&self.buffer, "count")),
        ];
        self.copy_pointer = pointer;
        self.copy_length = self.count_argument(Expression::member(&self.buffer, "count"));
        self
    }

    fn scope_call(&self) -> Expression {
        Expression::member(&self.collection, self.scope.method())
    }

    fn count_argument(&self, value: Expression) -> Expression {
        Expression::call(
            TypeName::uint(),
            [value].into_iter().collect::<ArgumentList>(),
        )
    }
}

impl ReceivedVector {
    pub fn setup(&self) -> Vec<Statement> {
        self.setup.clone()
    }

    pub fn value(&self) -> Expression {
        self.value.clone()
    }
}

impl CopiedVector {
    pub fn statement(&self, call: Expression) -> Statement {
        let copy = self.borrowed.copy(&self.copy);
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.borrowed
                    .wrap(Statement::returns(copy).indented("    "), "", true),
            ]
            .join("\n"),
        )
    }

    pub fn success_statement(
        &self,
        call: Expression,
        success_out: &Identifier,
        empty_error: Expression,
    ) -> Statement {
        let copy = self.borrowed.copy(&self.copy);
        let store = Statement::assign(
            Expression::optional_chain_member(success_out, "pointee"),
            copy,
        );
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.borrowed.wrap(store.indented("    "), "", false),
                Statement::returns(empty_error).to_string(),
            ]
            .join("\n"),
        )
    }

    pub fn consume_statement<F>(&self, call: Expression, consume: F) -> Statement
    where
        F: FnOnce(Expression) -> Statement,
    {
        let copy = self.borrowed.copy(&self.copy);
        Statement::new(
            [
                Statement::let_value(&self.result, call).to_string(),
                self.borrowed
                    .wrap(consume(copy).indented("    "), "", false),
            ]
            .join("\n"),
        )
    }
}

impl Scope {
    fn method(self) -> &'static str {
        match self {
            Self::TypedBuffer => "withUnsafeBufferPointer",
            Self::RawBytes => "withUnsafeBytes",
        }
    }
}

impl ReturnedVector {
    pub fn ty(&self) -> TypeName {
        self.ty.clone()
    }

    pub fn body(&self, value: Expression, indent: &str, free: &Identifier) -> Result<String> {
        let buffer = GeneratedLocal::ReturnBuffer.identifier()?;
        let pointer = GeneratedLocal::ReturnBuffer.suffixed("ptr")?;
        let count = GeneratedLocal::ReturnBuffer.suffixed("count")?;
        let raw = GeneratedLocal::ReturnBuffer.suffixed("raw")?;
        let buffer_binding = match value == Expression::identifier(buffer.clone()) {
            true => None,
            false => Some(Statement::let_value(&buffer, value).indented(indent)),
        };
        Ok([
            buffer_binding,
            Some(
                Statement::defer(Expression::call(
                    free,
                    [Expression::identifier(buffer.clone())]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ))
                .indented(indent),
            ),
            Some(format!(
                "{indent}guard {}.len > 0, let {pointer} = {}.ptr else {{ return [] }}",
                buffer, buffer
            )),
            Some(
                Statement::let_value(
                    &count,
                    Expression::new(format!(
                        "Int({buffer}.len) / MemoryLayout<{}>.stride",
                        self.element
                    )),
                )
                .indented(indent),
            ),
            Some(
                Statement::let_value(
                    &raw,
                    Expression::call(
                        Expression::member(
                            Expression::call(
                                "UnsafeRawPointer",
                                [Expression::identifier(pointer)]
                                    .into_iter()
                                    .collect::<ArgumentList>(),
                            ),
                            "assumingMemoryBound",
                        ),
                        [Expression::labeled("to", self.element.clone().metatype())]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    ),
                )
                .indented(indent),
            ),
            Some(self.return_value(Expression::identifier(raw), count, indent)?),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n"))
    }

    fn return_value(&self, raw: Expression, count: Identifier, indent: &str) -> Result<String> {
        match &self.decode {
            Decode::Primitive => Ok(Statement::returns(Expression::call(
                "Array",
                [Expression::call(
                    "UnsafeBufferPointer",
                    [
                        Expression::labeled("start", raw),
                        Expression::labeled("count", Expression::identifier(count)),
                    ]
                    .into_iter()
                    .collect::<ArgumentList>(),
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            ))
            .indented(indent)),
            Decode::Record { swift } => {
                let index = Identifier::parse("index")?;
                let value = GeneratedLocal::ReturnBuffer.suffixed("value")?;
                Ok([
                    format!(
                        "{indent}var {value}: {} = []",
                        TypeName::array(swift.clone())
                    ),
                    format!("{indent}{value}.reserveCapacity({count})"),
                    format!("{indent}for {index} in 0..<{count} {{"),
                    format!(
                        "{indent}    {value}.append({swift}(fromC: {}))",
                        Expression::subscript(raw, index)
                    ),
                    format!("{indent}}}"),
                    Statement::returns(value).indented(indent),
                ]
                .join("\n"))
            }
        }
    }
}
