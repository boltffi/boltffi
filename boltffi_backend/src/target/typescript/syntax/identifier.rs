use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

use super::Syntax;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MemberName(String);

impl Identifier {
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        match Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            true => Ok(Self(identifier)),
            false => Err(Error::InvalidTypeScriptIdentifier { identifier }),
        }
    }

    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        match Syntax::keyword(&identifier) {
            true => Self::parse(format!("_{identifier}")),
            false => Self::parse(identifier),
        }
    }

    pub fn known(identifier: &'static str) -> Self {
        Self::parse(identifier).expect("static TypeScript identifier must be valid")
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        characters
            .next()
            .is_some_and(|character| matches!(character, '_' | '$') || character.is_alphabetic())
            && characters
                .all(|character| matches!(character, '_' | '$') || character.is_alphanumeric())
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl MemberName {
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        match Identifier::valid(&name) {
            true => Ok(Self(name)),
            false => Err(Error::InvalidTypeScriptIdentifier { identifier: name }),
        }
    }
}

impl fmt::Display for MemberName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Identifier {}
impl sealed::SyntaxFragment for MemberName {}
