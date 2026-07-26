use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

/// C# syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A valid C# identifier, including verbatim keyword identifiers.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);

/// C# type syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeFragment(String);

/// C# expression syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

/// C# statement or member-declaration syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

/// C# literal syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

/// C# argument list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "abstract",
        "as",
        "base",
        "bool",
        "break",
        "byte",
        "case",
        "catch",
        "char",
        "checked",
        "class",
        "const",
        "continue",
        "decimal",
        "default",
        "delegate",
        "do",
        "double",
        "else",
        "enum",
        "event",
        "explicit",
        "extern",
        "false",
        "finally",
        "fixed",
        "float",
        "for",
        "foreach",
        "goto",
        "if",
        "implicit",
        "in",
        "int",
        "interface",
        "internal",
        "is",
        "lock",
        "long",
        "namespace",
        "new",
        "null",
        "object",
        "operator",
        "out",
        "override",
        "params",
        "private",
        "protected",
        "public",
        "readonly",
        "ref",
        "return",
        "sbyte",
        "sealed",
        "short",
        "sizeof",
        "stackalloc",
        "static",
        "string",
        "struct",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "uint",
        "ulong",
        "unchecked",
        "unsafe",
        "ushort",
        "using",
        "virtual",
        "void",
        "volatile",
        "while",
    ];

    type Identifier = Identifier;
    type Type = TypeFragment;
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
    /// Parses a non-keyword C# identifier.
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        match Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            true => Ok(Self(identifier)),
            false => Err(Error::InvalidCSharpIdentifier { identifier }),
        }
    }

    /// Escapes a C# keyword as a verbatim identifier.
    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if !Self::valid(&identifier) {
            return Err(Error::InvalidCSharpIdentifier { identifier });
        }
        match Syntax::keyword(&identifier) {
            true => Ok(Self(format!("@{identifier}"))),
            false => Ok(Self(identifier)),
        }
    }

    /// Returns the rendered identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        let Some(first) = characters.next() else {
            return false;
        };
        (first == '_' || first.is_alphabetic())
            && characters.all(|character| character == '_' || character.is_alphanumeric())
    }
}

impl sealed::SyntaxFragment for TypeFragment {}

impl fmt::Display for TypeFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TypeFragment {
    pub(crate) fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }

    pub(crate) fn void() -> Self {
        Self::new("void")
    }
}

impl sealed::SyntaxFragment for Expression {}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Expression {
    pub(crate) fn new(expression: impl Into<String>) -> Self {
        Self(expression.into())
    }

    pub(crate) fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub(crate) fn member(target: Identifier, member: Identifier) -> Self {
        Self(format!("{target}.{member}"))
    }

    pub(crate) fn call(callee: Self, arguments: ArgumentList) -> Self {
        Self(format!("{callee}({arguments})"))
    }
}

impl sealed::SyntaxFragment for Statement {}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Statement {
    pub(crate) fn new(statement: impl Into<String>) -> Self {
        Self(statement.into())
    }

    pub(crate) fn indented(&self, spaces: usize) -> String {
        let prefix = " ".repeat(spaces);
        self.0
            .lines()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl sealed::SyntaxFragment for Literal {}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Literal {
    pub(crate) fn string(value: &str) -> Self {
        let mut escaped = String::with_capacity(value.len());
        for character in value.chars() {
            match character {
                '\0' => escaped.push_str("\\0"),
                '\x07' => escaped.push_str("\\a"),
                '\x08' => escaped.push_str("\\b"),
                '\x0C' => escaped.push_str("\\f"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                '\x0B' => escaped.push_str("\\v"),
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                character
                    if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}') =>
                {
                    escaped.push_str(&format!("\\u{:04X}", u32::from(character)));
                }
                character => escaped.push(character),
            }
        }
        Self(format!("\"{escaped}\""))
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
    pub(crate) fn new(arguments: impl IntoIterator<Item = Expression>) -> Self {
        Self(arguments.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_escapes_keywords() {
        assert_eq!(Identifier::escape("event").unwrap().as_str(), "@event");
        assert_eq!(Identifier::escape("value").unwrap().as_str(), "value");
    }

    #[test]
    fn string_literal_escapes_csharp_delimiters() {
        assert_eq!(Literal::string("a\\b\"c").to_string(), "\"a\\\\b\\\"c\"");
    }

    #[test]
    fn string_literal_escapes_csharp_control_characters() {
        assert_eq!(
            Literal::string("\0\x07\x08\x0C\n\r\t\x0B\x1F\x7F\u{85}\u{2028}\u{2029}").to_string(),
            "\"\\0\\a\\b\\f\\n\\r\\t\\v\\u001F\\u007F\\u0085\\u2028\\u2029\""
        );
    }

    #[test]
    fn string_literal_preserves_printable_unicode() {
        assert_eq!(Literal::string("caf\u{e9}").to_string(), "\"caf\u{e9}\"");
    }
}
