use std::fmt::{self, Write};

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

/// Kotlin syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A valid Kotlin identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier(String);

/// Kotlin type syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeName(String);

/// Kotlin expression syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

/// Kotlin statement syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

/// Kotlin literal syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

/// Kotlin argument-list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "as",
        "break",
        "class",
        "continue",
        "do",
        "else",
        "false",
        "for",
        "fun",
        "if",
        "in",
        "interface",
        "is",
        "null",
        "object",
        "package",
        "return",
        "super",
        "this",
        "throw",
        "true",
        "try",
        "typealias",
        "typeof",
        "val",
        "var",
        "when",
        "while",
        "by",
        "catch",
        "constructor",
        "delegate",
        "dynamic",
        "field",
        "file",
        "finally",
        "get",
        "import",
        "init",
        "param",
        "property",
        "receiver",
        "set",
        "setparam",
        "where",
        "actual",
        "abstract",
        "annotation",
        "companion",
        "const",
        "crossinline",
        "data",
        "enum",
        "expect",
        "external",
        "final",
        "infix",
        "inline",
        "inner",
        "internal",
        "lateinit",
        "noinline",
        "open",
        "operator",
        "out",
        "override",
        "private",
        "protected",
        "public",
        "reified",
        "sealed",
        "suspend",
        "tailrec",
        "vararg",
    ];

    type Identifier = Identifier;
    type Type = TypeName;
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
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            Ok(Self(identifier))
        } else {
            Err(Error::InvalidKotlinIdentifier { identifier })
        }
    }

    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Syntax::keyword(&identifier) {
            Ok(Self(format!("`{identifier}`")))
        } else {
            Self::parse(identifier)
        }
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl sealed::SyntaxFragment for TypeName {}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TypeName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn qualified(package: impl fmt::Display, name: Self) -> Self {
        Self::new(format!("{package}.{name}"))
    }

    pub fn unit() -> Self {
        Self::new("Unit")
    }

    pub fn boolean() -> Self {
        Self::new("Boolean")
    }

    pub fn string() -> Self {
        Self::new("String")
    }

    pub fn byte() -> Self {
        Self::new("Byte")
    }

    pub fn ubyte() -> Self {
        Self::new("UByte")
    }

    pub fn short() -> Self {
        Self::new("Short")
    }

    pub fn ushort() -> Self {
        Self::new("UShort")
    }

    pub fn int() -> Self {
        Self::new("Int")
    }

    pub fn uint() -> Self {
        Self::new("UInt")
    }

    pub fn long() -> Self {
        Self::new("Long")
    }

    pub fn ulong() -> Self {
        Self::new("ULong")
    }

    pub fn float() -> Self {
        Self::new("Float")
    }

    pub fn double() -> Self {
        Self::new("Double")
    }

    pub fn byte_array(nullable: bool) -> Self {
        Self::new(match nullable {
            true => "ByteArray?",
            false => "ByteArray",
        })
    }

    pub fn parameterized(base: impl fmt::Display, arguments: Vec<Self>) -> Self {
        let arguments = arguments
            .into_iter()
            .map(|argument| argument.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Self::new(format!("{base}<{arguments}>"))
    }

    pub fn list(element: Self) -> Self {
        Self::new(format!("List<{element}>"))
    }

    pub fn map(key: Self, value: Self) -> Self {
        Self::new(format!("Map<{key}, {value}>"))
    }

    pub fn result(ok: Self, err: Self) -> Self {
        Self::new(format!("BoltFFIResult<{ok}, {err}>"))
    }

    pub fn function(parameters: Vec<Self>, returns: Self) -> Self {
        let parameters = parameters
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Self::new(format!("(({parameters}) -> {returns})"))
    }

    pub fn nullable(self) -> Self {
        Self::new(format!("{self}?"))
    }
}

impl sealed::SyntaxFragment for Expression {}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Expression {
    pub fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub fn integer(value: impl Into<i128>) -> Self {
        Self(value.into().to_string())
    }

    pub fn long(value: impl Into<i128>) -> Self {
        Self(format!("{}L", value.into()))
    }

    pub fn unsigned_long(value: impl Into<u128>) -> Self {
        Self(format!("{}uL", value.into()))
    }

    pub fn null() -> Self {
        Self("null".to_owned())
    }

    pub fn bool(value: bool) -> Self {
        Self(value.to_string())
    }

    pub fn literal(literal: Literal) -> Self {
        Self(literal.to_string())
    }

    pub fn float(value: f64, single_precision: bool) -> Self {
        let mut literal = value.to_string();
        if !literal.contains('.') && !literal.contains('e') && !literal.contains('E') {
            literal.push_str(".0");
        }
        if single_precision {
            literal.push('f');
        }
        Self(literal)
    }

    pub fn this() -> Self {
        Self("this".to_owned())
    }

    pub fn call(receiver: impl fmt::Display, method: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{receiver}.{method}({arguments})"))
    }

    pub fn invoke(function: impl fmt::Display, arguments: ArgumentList) -> Self {
        Self(format!("{function}({arguments})"))
    }

    pub fn construct(ty: TypeName, arguments: ArgumentList) -> Self {
        Self(format!("{ty}({arguments})"))
    }

    pub fn throwing(value: Self) -> Self {
        Self(format!("throw {value}"))
    }

    pub fn try_catch(self, error: Identifier, ty: TypeName, body: Self) -> Self {
        Self(format!(
            "try {{ {self} }} catch ({error}: {ty}) {{ {body} }}"
        ))
    }

    pub fn property(receiver: impl fmt::Display, property: Identifier) -> Self {
        Self(format!("{receiver}.{property}"))
    }

    pub fn safe_property(receiver: impl fmt::Display, property: Identifier) -> Self {
        Self(format!("{receiver}?.{property}"))
    }

    pub fn safe_call(
        receiver: impl fmt::Display,
        method: Identifier,
        arguments: ArgumentList,
    ) -> Self {
        Self(format!("{receiver}?.{method}({arguments})"))
    }

    pub fn add(self, other: Self) -> Self {
        Self(format!("{self} + {other}"))
    }

    pub fn multiply(self, other: Self) -> Self {
        Self(format!("{self} * {other}"))
    }

    pub fn divide(self, other: Self) -> Self {
        Self(format!("{self} / {other}"))
    }

    pub fn parenthesized(self) -> Self {
        Self(format!("({self})"))
    }

    pub fn not_equal(self, other: Self) -> Self {
        Self(format!("{self} != {other}"))
    }

    pub fn equal(self, other: Self) -> Self {
        Self(format!("{self} == {other}"))
    }

    pub fn conditional(condition: Self, then_value: Self, else_value: Self) -> Self {
        Self(format!("if ({condition}) {then_value} else {else_value}"))
    }

    pub fn lambda_expression(parameters: Vec<Identifier>, body: Self) -> Self {
        Self::lambda(parameters, body)
    }

    pub fn lambda_statement(parameters: Vec<Identifier>, body: Statement) -> Self {
        Self::lambda(parameters, body)
    }

    pub fn sum_of(self, parameter: Identifier, body: Self) -> Self {
        Self(format!(
            "{self}.sumOf {{ {parameter} -> ({body}).toInt() }}"
        ))
    }

    pub fn map(self, parameter: Identifier, body: Self) -> Self {
        Self(format!("{self}.map {{ {parameter} -> {body} }}"))
    }

    pub fn list(count: Self, body: Self) -> Self {
        Self(format!("List({count}) {{ {body} }}"))
    }

    pub fn indexed_list(count: Self, index: Identifier, body: Self) -> Self {
        Self(format!("List({count}) {{ {index} -> {body} }}"))
    }

    pub fn optional_size(self, parameter: Identifier, body: Self) -> Self {
        Self(format!(
            "1 + ({self}?.let {{ {parameter} -> {body} }} ?: 0)"
        ))
    }

    pub fn result_size(self, parameter: Identifier, ok: Self, err: Self) -> Self {
        Self(format!(
            "{self}.wireSize({{ {parameter} -> {ok} }}, {{ {parameter} -> {err} }})"
        ))
    }

    pub fn map_size(
        self,
        key: Identifier,
        key_body: Self,
        value: Identifier,
        value_body: Self,
    ) -> Self {
        Self(format!(
            "{self}.wireSize({{ {key} -> {key_body} }}, {{ {value} -> {value_body} }})"
        ))
    }

    pub fn result_fold(
        self,
        ok: Identifier,
        ok_body: Self,
        err: Identifier,
        err_body: Self,
    ) -> Self {
        Self(format!(
            "{self}.fold({{ {ok} -> {ok_body} }}, {{ {err} -> {err_body} }})"
        ))
    }

    pub fn run(statements: Vec<Statement>, result: Self) -> Self {
        match statements.is_empty() {
            true => result,
            false => Self(format!(
                "run {{ {}; {result} }}",
                statements
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }

    pub fn or_else(self, fallback: Self) -> Self {
        Self(format!("{self} ?: {fallback}"))
    }

    pub fn let_or_else(self, parameter: Identifier, body: Self, fallback: Self) -> Self {
        Self(format!(
            "{self}?.let {{ {parameter} -> {body} }} ?: {fallback}"
        ))
    }

    pub fn throw_illegal_state(message: Literal) -> Self {
        Self(format!("throw IllegalStateException({message})"))
    }

    pub fn throw_illegal_argument(message: Literal) -> Self {
        Self(format!("throw IllegalArgumentException({message})"))
    }

    pub fn convert(self, method: Identifier) -> Self {
        Self(format!("{self}.{method}()"))
    }

    fn lambda(parameters: Vec<Identifier>, body: impl fmt::Display) -> Self {
        let parameters = parameters
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Self(format!("{{ {parameters} -> {body} }}"))
    }
}

impl sealed::SyntaxFragment for Statement {}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Statement {
    pub fn value(name: Identifier, value: Expression) -> Self {
        Self(format!("val {name} = {value}"))
    }

    pub fn return_value(value: Expression) -> Self {
        Self(format!("return {value}"))
    }

    pub fn expression(value: Expression) -> Self {
        Self(value.to_string())
    }

    pub fn with_return_value(mut statements: Vec<Self>) -> Vec<Self> {
        if let Some(statement) = statements.last_mut() {
            statement.0 = format!("return {}", statement.0);
        }
        statements
    }
}

impl sealed::SyntaxFragment for Literal {}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Literal {
    pub fn string(value: &str) -> Self {
        Self::render(value, true)
    }

    pub fn interpolated_string(value: &str) -> Self {
        Self::render(value, false)
    }

    fn render(value: &str, escape_dollar: bool) -> Self {
        let mut literal = String::with_capacity(value.len() + 2);
        literal.push('"');
        value.chars().for_each(|character| match character {
            '\t' => literal.push_str("\\t"),
            '\u{8}' => literal.push_str("\\b"),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '$' if escape_dollar => literal.push_str("\\$"),
            '\u{2028}' | '\u{2029}' => Self::push_unicode_escape(&mut literal, character),
            character if character.is_control() => {
                Self::push_unicode_escape(&mut literal, character)
            }
            character => literal.push(character),
        });
        literal.push('"');
        Self(literal)
    }

    fn push_unicode_escape(literal: &mut String, character: char) {
        let mut units = [0; 2];
        character.encode_utf16(&mut units).iter().for_each(|unit| {
            write!(literal, "\\u{unit:04x}").expect("writing a Kotlin Unicode escape")
        });
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
    fn from_expressions(expressions: impl IntoIterator<Item = Expression>) -> Self {
        Self(expressions.into_iter().collect())
    }
}

impl FromIterator<Expression> for ArgumentList {
    fn from_iter<T: IntoIterator<Item = Expression>>(expressions: T) -> Self {
        Self::from_expressions(expressions)
    }
}

#[cfg(test)]
mod tests {
    use super::Literal;

    #[test]
    fn string_escapes_interpolation_markers() {
        assert_eq!(
            Literal::string("native-$core\0\u{8}\u{c}\n\r\"\\\u{2028}😀").to_string(),
            "\"native-\\$core\\u0000\\b\\u000c\\n\\r\\\"\\\\\\u2028😀\""
        );
    }

    #[test]
    fn interpolated_string_preserves_interpolation_markers() {
        assert_eq!(
            Literal::interpolated_string("unknown tag: $tag").to_string(),
            "\"unknown tag: $tag\""
        );
    }
}
