use std::fmt;

use crate::core::{Error, LanguageSyntax, Result};

use super::syntax::Syntax;

/// A valid C identifier.
///
/// Keywords are rejected by [`Identifier::parse`] and escaped by
/// [`Identifier::escape`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(String);

impl Identifier {
    /// Creates a C identifier from already escaped text.
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            Ok(Self(identifier))
        } else {
            Err(Error::InvalidCIdentifier { identifier })
        }
    }

    /// Creates a valid portable ABI identifier, appending an underscore when the
    /// input is reserved by C or C++ (the header is compilable under both).
    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        match Syntax::reserved(&identifier) {
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
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
