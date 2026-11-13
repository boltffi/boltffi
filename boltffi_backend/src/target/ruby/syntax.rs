use std::fmt;

use ruby_prism::StringQuery;

use crate::{
    bridge::c,
    core::{Error, LanguageSyntax, Result, syntax::sealed},
};

/// Ruby syntax fragment family.
///
/// The Ruby target emits a C extension, so its types, expressions,
/// statements, and argument lists are the C bridge fragments. The Ruby-facing
/// names (method names, `Data` member names, and constant names) and the
/// literals in the Ruby package files use the fragments below. Each fragment
/// gets its constructor with the renderer that first builds it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A Ruby method or `Data` member name, such as `echo_string`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);

/// A Ruby constant name, such as `Point`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Constant(String);

/// A Ruby constant path, such as `MyLib::Native`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ConstantPath(Vec<Constant>);

/// A double-quoted Ruby string literal.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

impl LanguageSyntax for Syntax {
    // Prism decides which names are keywords, through `StringQuery::is_keyword`.
    // The keyword set follows the `ruby-prism` version that this crate pins.
    // The table holds the names that Ruby reserves beyond Prism's keywords.
    // `__END__` is not a keyword, but Ruby treats the marker as end of source.
    const KEYWORDS: &'static [&'static str] = &["__END__"];

    type Identifier = Identifier;
    type Type = c::TypeFragment;
    type Expr = c::Expression;
    type Stmt = c::Statement;
    type Literal = Literal;
    type Arguments = c::ArgumentList;

    fn keyword(identifier: &str) -> bool {
        Self::KEYWORDS.contains(&identifier)
            || StringQuery::new(identifier.as_bytes()).is_keyword() == Ok(true)
    }
}

impl sealed::LanguageSyntax for Syntax {}

impl Constant {
    /// Parses a Ruby constant name made of ASCII word characters.
    /// Rejects keywords because the constant can start a Ruby expression.
    pub fn parse(constant: impl Into<String>) -> Result<Self> {
        let constant = constant.into();
        let mut characters = constant.chars();
        let valid = characters
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
            && !Syntax::keyword(&constant);
        if valid {
            Ok(Self(constant))
        } else {
            Err(Error::InvalidRubyIdentifier {
                identifier: constant,
            })
        }
    }

    /// Returns the constant text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ConstantPath {
    /// Parses a `::`-separated Ruby constant path.
    pub fn parse(path: &str) -> Result<Self> {
        path.split("::")
            .map(Constant::parse)
            .collect::<Result<Vec<_>>>()
            .map(Self)
    }

    /// Creates a path with one constant.
    pub fn single(constant: Constant) -> Self {
        Self(vec![constant])
    }

    /// Returns the constants from the outermost to the innermost.
    pub fn segments(&self) -> &[Constant] {
        &self.0
    }
}

impl Literal {
    /// Creates a double-quoted Ruby string literal.
    ///
    /// Control characters become escapes, so the generated source stays a
    /// plain text file and line-ending conversion cannot change the value.
    pub fn string(value: &str) -> Self {
        let escaped = value.chars().fold(String::new(), |mut literal, character| {
            match character {
                '"' => literal.push_str("\\\""),
                '\\' => literal.push_str("\\\\"),
                '#' => literal.push_str("\\#"),
                '\n' => literal.push_str("\\n"),
                '\r' => literal.push_str("\\r"),
                '\t' => literal.push_str("\\t"),
                control if control.is_control() => {
                    literal.push_str(&format!("\\u{:04X}", u32::from(control)));
                }
                _ => literal.push(character),
            }
            literal
        });
        Self(format!("\"{escaped}\""))
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for ConstantPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self
            .0
            .iter()
            .map(Constant::as_str)
            .collect::<Vec<_>>()
            .join("::");
        formatter.write_str(&path)
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Identifier {}
impl sealed::SyntaxFragment for Literal {}

#[cfg(test)]
mod tests {
    use crate::core::LanguageSyntax;

    use super::{Constant, ConstantPath, Literal, Syntax};

    #[test]
    fn keyword_predicate_rejects_keywords_and_source_markers() {
        for reserved in [
            "class",
            "defined?",
            "nil",
            "self",
            "__FILE__",
            "__LINE__",
            "__ENCODING__",
            "__END__",
        ] {
            assert!(Syntax::keyword(reserved), "accepted {reserved}");
        }
        for ordinary in ["class?", "defined", "Nil", "begin_", "Point", "_1", "it"] {
            assert!(!Syntax::keyword(ordinary), "reserved {ordinary}");
        }
    }

    #[test]
    fn keyword_predicate_does_not_classify_other_source_as_a_keyword() {
        for source in ["", "class Point; end", "1", "Point::Native", "name + other"] {
            assert!(!Syntax::keyword(source), "reserved {source}");
        }
    }

    #[test]
    fn constants_reject_invalid_ruby_names() {
        assert!(Constant::parse("point").is_err());
        assert!(Constant::parse("My-Lib").is_err());
        assert!(ConstantPath::parse("MyLib::").is_err());
        assert_eq!(
            ConstantPath::parse("MyLib::Native").unwrap().to_string(),
            "MyLib::Native"
        );
    }

    #[test]
    fn constants_reject_ruby_keywords() {
        for keyword in ["BEGIN", "END"] {
            assert!(Constant::parse(keyword).is_err(), "accepted {keyword}");
            assert!(ConstantPath::parse(&format!("{keyword}::Native")).is_err());
        }
        assert_eq!(Constant::parse("Begin").unwrap().as_str(), "Begin");
        assert_eq!(Constant::parse("End").unwrap().as_str(), "End");
    }

    #[test]
    fn string_literals_escape_interpolation() {
        assert_eq!(Literal::string("a#{b}\"c").to_string(), "\"a\\#{b}\\\"c\"");
    }

    #[test]
    fn string_literals_escape_control_characters() {
        assert_eq!(
            Literal::string("\0\r\t\u{1f}\u{7f}\u{85}1é").to_string(),
            "\"\\u0000\\r\\t\\u001F\\u007F\\u00851é\""
        );
        assert!(!Literal::string("\r\n").to_string().contains('\r'));
    }
}
