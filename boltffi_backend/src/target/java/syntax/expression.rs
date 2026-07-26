use std::fmt;

use crate::core::syntax::sealed;

use super::{Identifier, StringLiteral, TypeName};

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

    pub fn static_call(ty: TypeName, method: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{ty}.{method}({arguments})"))
    }

    pub fn call(self, method: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{self}.{method}({arguments})"))
    }

    pub fn construct(ty: TypeName, arguments: ArgumentList) -> Self {
        Self(format!("new {ty}({arguments})"))
    }

    pub fn array(element: TypeName, length: Self) -> Self {
        Self(format!("new {element}[{length}]"))
    }

    pub fn member(self, member: Identifier) -> Self {
        Self(format!("{self}.{member}"))
    }

    pub fn lambda(parameters: impl IntoIterator<Item = Identifier>, expression: Self) -> Self {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Self(format!("({parameters}) -> {expression}"))
    }

    pub fn lambda_statement(
        parameters: impl IntoIterator<Item = Identifier>,
        statement: Statement,
    ) -> Self {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Self(format!("({parameters}) -> {{ {statement} }}"))
    }

    pub fn integer(value: u64) -> Self {
        Self(value.to_string())
    }

    pub fn signed_integer(value: i128) -> Self {
        Self(value.to_string())
    }

    pub fn long(value: i64) -> Self {
        Self(format!("{value}L"))
    }

    pub fn hexadecimal_long(value: u64) -> Self {
        Self(format!("0x{value:016X}L"))
    }

    pub fn float32(value: f32) -> Self {
        match value.is_finite() {
            true => Self(format!("{value:?}f")),
            false => Self(format!("Float.intBitsToFloat(0x{:08X})", value.to_bits())),
        }
    }

    pub fn float64(value: f64) -> Self {
        match value.is_finite() {
            true => Self(format!("{value:?}")),
            false => Self(format!(
                "Double.longBitsToDouble(0x{:016X}L)",
                value.to_bits()
            )),
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self(value.to_string())
    }

    pub fn string(value: StringLiteral) -> Self {
        Self(value.to_string())
    }

    pub fn null() -> Self {
        Self("null".to_owned())
    }

    pub fn equal(self, other: Self) -> Self {
        Self(format!("{self} == {other}"))
    }

    pub fn add(self, other: Self) -> Self {
        Self(format!("({self} + {other})"))
    }

    pub fn multiply(self, other: Self) -> Self {
        Self(format!("({self} * {other})"))
    }

    pub fn not_equal(self, other: Self) -> Self {
        Self(format!("{self} != {other}"))
    }

    pub fn conditional(self, then_value: Self, else_value: Self) -> Self {
        Self(format!("({self} ? {then_value} : {else_value})"))
    }

    pub fn cast(ty: crate::target::java::primitive::Primitive, value: Self) -> Self {
        Self(format!("({ty}) ({value})"))
    }
}

impl Statement {
    pub fn value(ty: TypeName, name: Identifier, value: Expression) -> Self {
        Self(format!("{ty} {name} = {value};"))
    }

    pub fn expression(expression: Expression) -> Self {
        Self(format!("{expression};"))
    }

    pub fn return_value(value: Expression) -> Self {
        Self(format!("return {value};"))
    }

    pub fn throw_value(value: Expression) -> Self {
        Self(format!("throw {value};"))
    }

    pub fn try_finally(body: Vec<Self>, cleanup: Vec<Self>) -> Self {
        Self(format!(
            "try {{\n{}\n}} finally {{\n{}\n}}",
            Self::indented(body),
            Self::indented(cleanup),
        ))
    }

    pub fn try_catch(
        body: Vec<Self>,
        exception: TypeName,
        name: Identifier,
        recovery: Vec<Self>,
    ) -> Self {
        Self(format!(
            "try {{\n{}\n}} catch ({exception} {name}) {{\n{}\n}}",
            Self::indented(body),
            Self::indented(recovery),
        ))
    }

    fn indented(statements: Vec<Self>) -> String {
        statements
            .into_iter()
            .flat_map(|statement| {
                statement
                    .0
                    .lines()
                    .map(|line| format!("    {line}"))
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

#[cfg(test)]
mod tests {
    use crate::target::java::{
        JavaVersion,
        syntax::{Identifier, TypeIdentifier},
    };

    use super::{ArgumentList, Expression, Statement, TypeName};

    #[test]
    fn composes_native_calls_from_typed_fragments() {
        let call = Expression::static_call(
            TypeName::named(TypeIdentifier::known("Native", JavaVersion::JAVA_8)),
            Identifier::known("add"),
            [
                Expression::identifier(Identifier::known("left")),
                Expression::identifier(Identifier::known("right")),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        );
        assert_eq!(call.to_string(), "Native.add(left, right)");
        assert_eq!(
            Statement::return_value(call).to_string(),
            "return Native.add(left, right);"
        );
    }

    #[test]
    fn preserves_conditional_precedence_inside_arithmetic() {
        let capacity = Expression::integer(1).add(
            Expression::identifier(Identifier::known("present"))
                .conditional(Expression::integer(4), Expression::integer(0)),
        );

        assert_eq!(capacity.to_string(), "(1 + (present ? 4 : 0))");
    }
}
