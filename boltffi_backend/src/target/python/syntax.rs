use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

/// Python syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A valid Python identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier(String);

/// Python type annotation syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeAnnotation(String);

/// Python expression syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

/// Python statement syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

/// Python literal syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

/// Python argument list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

/// A Python call expression.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CallExpression {
    callee: Expression,
    positional: Vec<Expression>,
    keywords: Vec<KeywordArgument>,
}

/// A Python keyword argument.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeywordArgument {
    name: Identifier,
    value: Expression,
}

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "case", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "match", "nonlocal", "not", "or", "pass", "raise",
        "return", "try", "type", "while", "with", "yield",
    ];

    type Identifier = Identifier;
    type Type = TypeAnnotation;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl sealed::SyntaxFragment for Identifier {}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Identifier {
    /// Parses a Python identifier.
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        match Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            true => Ok(Self(identifier)),
            false => Err(Error::InvalidPythonIdentifier { identifier }),
        }
    }

    /// Escapes a Python keyword into a valid identifier.
    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        match Syntax::keyword(&identifier) {
            true => Self::parse(format!("{identifier}_")),
            false => Self::parse(identifier),
        }
    }

    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        let Some(first_character) = characters.next() else {
            return false;
        };
        (first_character == '_' || first_character.is_alphabetic())
            && characters.all(|character| character == '_' || character.is_alphanumeric())
    }
}

impl sealed::SyntaxFragment for TypeAnnotation {}

impl fmt::Display for TypeAnnotation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TypeAnnotation {
    fn new(annotation: impl Into<String>) -> Self {
        Self(annotation.into())
    }

    pub(crate) fn none() -> Self {
        Self::new("None")
    }

    pub(crate) fn bool() -> Self {
        Self::new("bool")
    }

    pub(crate) fn int() -> Self {
        Self::new("int")
    }

    pub(crate) fn float() -> Self {
        Self::new("float")
    }

    pub(crate) fn string() -> Self {
        Self::new("str")
    }

    pub(crate) fn uuid() -> Self {
        Self::new("uuid.UUID")
    }

    pub(crate) fn bytes() -> Self {
        Self::new("bytes")
    }

    pub(crate) fn object() -> Self {
        Self::new("object")
    }

    pub(crate) fn callable_any_object() -> Self {
        Self::new("Callable[..., object]")
    }

    pub(crate) fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub(crate) fn optional(inner: Self) -> Self {
        Self::new(format!("{inner} | None"))
    }

    pub(crate) fn union(left: Self, right: Self) -> Self {
        Self::new(format!("{left} | {right}"))
    }

    pub(crate) fn list(element: Self) -> Self {
        Self::new(format!("list[{element}]"))
    }

    pub(crate) fn sequence(element: Self) -> Self {
        Self::new(format!("Sequence[{element}]"))
    }

    pub(crate) fn dict(key: Self, value: Self) -> Self {
        Self::new(format!("dict[{key}, {value}]"))
    }

    pub(crate) fn tuple(elements: impl IntoIterator<Item = Self>) -> Self {
        Self::new(format!(
            "tuple[{}]",
            elements
                .into_iter()
                .map(|element| element.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub(crate) fn result_pair(ok: Self, err: Self) -> Self {
        Self::tuple([Self::bool(), Self::union(ok, err)])
    }

    /// Returns the annotation text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl sealed::SyntaxFragment for Expression {}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Expression {
    fn new(expression: impl Into<String>) -> Self {
        Self(expression.into())
    }

    /// Creates an expression from an identifier.
    pub fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub(crate) fn literal(literal: Literal) -> Self {
        Self(literal.to_string())
    }

    /// Creates an attribute access expression.
    pub fn attribute(receiver: Expression, attribute: Identifier) -> Self {
        Self(format!("{receiver}.{attribute}"))
    }

    pub(crate) fn subscript(receiver: Expression, index: Expression) -> Self {
        Self(format!("{receiver}[{index}]"))
    }

    /// Creates a call expression.
    pub fn call(call: CallExpression) -> Self {
        call.into_expression()
    }

    /// Creates a conditional expression.
    pub fn conditional(
        then_value: Expression,
        condition: Expression,
        else_value: Expression,
    ) -> Self {
        Self(format!("{then_value} if {condition} else {else_value}"))
    }

    pub(crate) fn binary(left: Expression, operator: &'static str, right: Expression) -> Self {
        Self(format!("({left} {operator} {right})"))
    }

    pub(crate) fn lambda(argument: Identifier, body: Expression) -> Self {
        Self(format!("lambda {argument}: {body}"))
    }

    pub(crate) fn no_arg_lambda(body: Expression) -> Self {
        Self(format!("lambda: {body}"))
    }

    pub(crate) fn await_value(value: Expression) -> Self {
        Self(format!("await {value}"))
    }

    pub(crate) fn empty_list() -> Self {
        Self::new("[]")
    }

    pub(crate) fn tuple(elements: impl IntoIterator<Item = Expression>) -> Self {
        Self::new(format!(
            "({},)",
            elements
                .into_iter()
                .map(|element| element.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

impl sealed::SyntaxFragment for Statement {}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Statement {
    /// Creates a return statement.
    pub fn return_value(value: Expression) -> Self {
        Self(format!("return {value}"))
    }

    /// Creates an expression statement.
    pub fn expression(value: Expression) -> Self {
        Self(value.to_string())
    }

    /// Creates an assignment statement.
    pub fn assign(target: Identifier, value: Expression) -> Self {
        Self(format!("{target} = {value}"))
    }

    /// Creates a multi-line assignment to a call expression.
    pub fn assign_call(target: Identifier, call: CallExpression) -> Vec<Self> {
        call.assignment_lines(target)
    }
}

impl sealed::SyntaxFragment for Literal {}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Literal {
    fn new(literal: impl Into<String>) -> Self {
        Self(literal.into())
    }

    pub(crate) fn integer(value: i128) -> Self {
        Self::new(value.to_string())
    }

    pub(crate) fn bool(value: bool) -> Self {
        Self::new(match value {
            true => "True",
            false => "False",
        })
    }

    pub(crate) fn none() -> Self {
        Self::new("None")
    }

    pub(crate) fn string(value: &str) -> Self {
        Self::new(format!("{value:?}"))
    }

    pub(crate) fn bytes_empty() -> Self {
        Self::new("b\"\"")
    }

    pub(crate) fn float(value: f64) -> Expression {
        if value.is_nan() {
            return Expression::call(
                CallExpression::new(Expression::identifier(
                    Identifier::parse("float").expect("float is a valid Python identifier"),
                ))
                .positional(Expression::literal(Self::string("nan"))),
            );
        }
        if value == f64::INFINITY {
            return Expression::call(
                CallExpression::new(Expression::identifier(
                    Identifier::parse("float").expect("float is a valid Python identifier"),
                ))
                .positional(Expression::literal(Self::string("inf"))),
            );
        }
        if value == f64::NEG_INFINITY {
            return Expression::call(
                CallExpression::new(Expression::identifier(
                    Identifier::parse("float").expect("float is a valid Python identifier"),
                ))
                .positional(Expression::literal(Self::string("-inf"))),
            );
        }
        if value == 0.0 && value.is_sign_negative() {
            return Expression::new("-0.0");
        }
        Expression::new(value.to_string())
    }
}

impl sealed::SyntaxFragment for ArgumentList {}

impl fmt::Display for ArgumentList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            &self
                .0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl ArgumentList {
    pub(crate) fn from_iter(expressions: impl IntoIterator<Item = Expression>) -> Self {
        Self(expressions.into_iter().collect())
    }
}

impl CallExpression {
    /// Creates a call expression for a callee.
    pub fn new(callee: Expression) -> Self {
        Self {
            callee,
            positional: Vec::new(),
            keywords: Vec::new(),
        }
    }

    /// Adds one positional argument.
    pub fn positional(mut self, value: Expression) -> Self {
        self.positional.push(value);
        self
    }

    /// Adds one keyword argument.
    pub fn keyword(mut self, name: Identifier, value: Expression) -> Self {
        self.keywords.push(KeywordArgument { name, value });
        self
    }

    /// Renders the call as an expression.
    pub fn into_expression(self) -> Expression {
        let arguments = self.arguments();
        Expression(format!("{}({arguments})", self.callee))
    }

    fn assignment_lines(self, target: Identifier) -> Vec<Statement> {
        std::iter::once(Statement(format!("{target} = {}(", self.callee)))
            .chain(
                self.positional
                    .into_iter()
                    .map(|argument| Statement(format!("    {argument},"))),
            )
            .chain(
                self.keywords.into_iter().map(|argument| {
                    Statement(format!("    {}={},", argument.name, argument.value))
                }),
            )
            .chain(std::iter::once(Statement(")".to_owned())))
            .collect()
    }

    fn arguments(&self) -> String {
        self.positional
            .iter()
            .map(ToString::to_string)
            .chain(
                self.keywords
                    .iter()
                    .map(|argument| format!("{}={}", argument.name, argument.value)),
            )
            .collect::<Vec<_>>()
            .join(", ")
    }
}
