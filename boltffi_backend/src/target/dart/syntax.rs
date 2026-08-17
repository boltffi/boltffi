use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeFragment(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeFragment,
}

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "abstract",
        "as",
        "assert",
        "async",
        "await",
        "base",
        "bool",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "covariant",
        "default",
        "deferred",
        "do",
        "double",
        "dynamic",
        "else",
        "enum",
        "export",
        "extends",
        "extension",
        "external",
        "factory",
        "false",
        "final",
        "finally",
        "for",
        "Function",
        "get",
        "hide",
        "if",
        "implements",
        "import",
        "in",
        "int",
        "interface",
        "is",
        "late",
        "library",
        "mixin",
        "new",
        "num",
        "null",
        "of",
        "on",
        "operator",
        "part",
        "required",
        "rethrow",
        "return",
        "sealed",
        "set",
        "show",
        "static",
        "super",
        "switch",
        "sync",
        "this",
        "throw",
        "true",
        "try",
        "type",
        "typedef",
        "var",
        "void",
        "when",
        "while",
        "with",
        "yield",
    ];

    type Identifier = Identifier;
    type Type = TypeFragment;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl Syntax {
    pub fn record<Element>(elements: impl IntoIterator<Item = Element>) -> String
    where
        Element: fmt::Display,
    {
        let elements = elements
            .into_iter()
            .map(|element| element.to_string())
            .collect::<Vec<_>>();
        match elements.as_slice() {
            [element] => format!("({element},)"),
            _ => format!("({})", elements.join(", ")),
        }
    }
}

macro_rules! text_fragment {
    ($name:ident) => {
        impl sealed::SyntaxFragment for $name {}

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

impl sealed::SyntaxFragment for Identifier {}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Identifier {
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            Ok(Self(identifier))
        } else {
            Err(Error::InvalidDartIdentifier { identifier })
        }
    }

    pub fn normalize(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if !Self::valid(&identifier) {
            return Err(Error::InvalidDartIdentifier { identifier });
        }
        match Syntax::keyword(&identifier) {
            true => Ok(Self(format!("${identifier}"))),
            false => Ok(Self(identifier)),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        characters.next().is_some_and(|character| {
            character == '_' || character == '$' || character.is_alphabetic()
        }) && characters
            .all(|character| character == '_' || character == '$' || character.is_alphanumeric())
    }
}

text_fragment!(TypeFragment);
text_fragment!(Expression);
text_fragment!(Statement);
text_fragment!(Literal);

impl TypeFragment {
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn optional(self) -> Self {
        Self::new(format!("{self}?"))
    }

    pub fn optional_function(self) -> Self {
        Self::new(format!("({self})?"))
    }

    pub fn future(self) -> Self {
        Self::new(format!("Future<{self}>"))
    }

    pub fn function(returns: Self, parameters: impl IntoIterator<Item = Self>) -> Self {
        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Self::new(format!("{returns} Function({parameters})"))
    }
}

impl Literal {
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }
}

impl Expression {
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Parameter {
    pub fn new(name: Identifier, ty: TypeFragment) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeFragment {
        &self.ty
    }
}

impl fmt::Display for Parameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.ty, self.name)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_normalizes_keywords_without_losing_the_source_name() {
        assert_eq!(Identifier::normalize("class").unwrap().as_str(), "$class");
        assert_eq!(
            Identifier::normalize("httpClient").unwrap().as_str(),
            "httpClient"
        );
    }

    #[test]
    fn record_syntax_preserves_single_element_shape() {
        assert_eq!(Syntax::record(["int"]), "(int,)");
        assert_eq!(Syntax::record(["int", "String"]), "(int, String)");
    }
}
