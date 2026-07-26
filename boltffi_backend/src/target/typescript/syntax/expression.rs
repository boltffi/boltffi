use std::fmt;

use crate::core::syntax::sealed;

use super::{Identifier, IntegerLiteral, PropertyKey, StringLiteral};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

impl Expression {
    pub fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub fn this() -> Self {
        Self("this".to_owned())
    }

    pub fn string(literal: StringLiteral) -> Self {
        Self(literal.to_string())
    }

    pub fn array(elements: impl IntoIterator<Item = Self>) -> Self {
        Self(format!(
            "[{}]",
            elements
                .into_iter()
                .map(|element| element.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub fn object(properties: impl IntoIterator<Item = (PropertyKey, Self)>) -> Self {
        Self(format!(
            "{{ {} }}",
            properties
                .into_iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub fn native_call(symbol: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("(_exports.{symbol} as Function)({arguments})"))
    }

    pub fn call(receiver: Self, method: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{receiver}.{method}({arguments})"))
    }

    pub fn invoke(function: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{function}({arguments})"))
    }

    pub fn lambda(expression: Self) -> Self {
        Self(format!("() => {expression}"))
    }

    pub fn parameter_lambda(parameter: Identifier, expression: Self) -> Self {
        Self(format!("({parameter}) => {expression}"))
    }

    pub fn parameters_lambda(
        parameters: impl IntoIterator<Item = Identifier>,
        expression: Self,
    ) -> Self {
        Self(format!(
            "({}) => {expression}",
            parameters
                .into_iter()
                .map(|parameter| parameter.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    pub fn statements_lambda(parameter: Identifier, statements: Vec<Statement>) -> Self {
        Self(format!(
            "({parameter}) => {{\n{}\n}}",
            Statement::indent(statements)
        ))
    }

    pub fn property(receiver: Self, property: Identifier) -> Self {
        Self(format!("{receiver}.{property}"))
    }

    pub fn static_call(ty: impl fmt::Display, method: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{ty}.{method}({arguments})"))
    }

    pub fn index(receiver: Self, index: u32) -> Self {
        Self(format!("{receiver}[{index}]"))
    }

    pub fn integer(value: u64) -> Self {
        Self(value.to_string())
    }

    pub fn bigint(value: u64) -> Self {
        Self(format!("{value}n"))
    }

    pub fn integer_literal(literal: IntegerLiteral) -> Self {
        Self(literal.to_string())
    }

    pub fn boolean(value: bool) -> Self {
        Self(value.to_string())
    }

    pub fn floating(value: f64) -> Self {
        Self(match value {
            value if value.is_nan() => "Number.NaN".to_owned(),
            value if value == f64::INFINITY => "Number.POSITIVE_INFINITY".to_owned(),
            value if value == f64::NEG_INFINITY => "Number.NEGATIVE_INFINITY".to_owned(),
            value if value == 0.0 && value.is_sign_negative() => "-0.0".to_owned(),
            value => format!("{value:?}"),
        })
    }

    pub fn null() -> Self {
        Self("null".to_owned())
    }

    pub fn nan() -> Self {
        Self("Number.NaN".to_owned())
    }

    pub fn await_value(self) -> Self {
        Self(format!("await {self}"))
    }

    pub fn add(self, other: Self) -> Self {
        Self(format!("({self} + {other})"))
    }

    pub fn multiply(self, other: Self) -> Self {
        Self(format!("({self} * {other})"))
    }

    pub fn strict_equal(self, other: Self) -> Self {
        Self(format!("{self} === {other}"))
    }

    pub fn strict_not_equal(self, other: Self) -> Self {
        Self(format!("{self} !== {other}"))
    }

    pub fn construct(ty: impl fmt::Display, arguments: ArgumentList) -> Self {
        Self(format!("new {ty}({arguments})"))
    }

    pub fn conditional(self, then_value: Self, else_value: Self) -> Self {
        Self(format!("({self} ? {then_value} : {else_value})"))
    }

    pub fn cast(self, ty: impl fmt::Display) -> Self {
        Self(format!("{self} as {ty}"))
    }

    pub fn not_zero(self) -> Self {
        Self(format!("{self} !== 0"))
    }
}

impl Statement {
    pub fn constant(name: Identifier, value: Expression) -> Self {
        Self(format!("const {name} = {value};"))
    }

    pub fn expression(expression: Expression) -> Self {
        Self(format!("{expression};"))
    }

    pub fn return_value(expression: Expression) -> Self {
        Self(format!("return {expression};"))
    }

    pub fn throwing(expression: Expression) -> Self {
        Self(format!("throw {expression};"))
    }

    pub fn throw_when(condition: Expression, expression: Expression) -> Self {
        Self(format!("if ({condition}) throw {expression};"))
    }

    pub fn when(condition: Expression, body: Vec<Self>) -> Self {
        Self(format!("if ({condition}) {{\n{}\n}}", Self::indent(body)))
    }

    pub fn try_finally(body: Vec<Self>, cleanup: Vec<Self>) -> Self {
        Self(format!(
            "try {{\n{}\n}} finally {{\n{}\n}}",
            Self::indent(body),
            Self::indent(cleanup),
        ))
    }

    fn indent(statements: Vec<Self>) -> String {
        statements
            .into_iter()
            .flat_map(|statement| {
                statement
                    .0
                    .lines()
                    .map(|line| format!("  {line}"))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl FromIterator<Expression> for ArgumentList {
    fn from_iter<T: IntoIterator<Item = Expression>>(expressions: T) -> Self {
        Self(expressions.into_iter().collect())
    }
}

impl IntoIterator for ArgumentList {
    type Item = Expression;
    type IntoIter = std::vec::IntoIter<Expression>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

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

impl sealed::SyntaxFragment for Expression {}
impl sealed::SyntaxFragment for Statement {}
impl sealed::SyntaxFragment for ArgumentList {}
