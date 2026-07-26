use std::fmt;

use crate::{
    core::{Error, LanguageSyntax, Result, syntax::sealed},
    target::java::JavaVersion,
};

use super::{Syntax, unicode::JavaIdentifiers};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier {
    spelling: String,
    restricted_since: Option<JavaVersion>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeIdentifier {
    identifier: Identifier,
    restricted_since: Option<JavaVersion>,
}

impl Identifier {
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        Self::parse_for(identifier, JavaVersion::JAVA_8)
    }

    pub fn parse_for(identifier: impl Into<String>, version: JavaVersion) -> Result<Self> {
        version.validate()?;
        let source = identifier.into();
        let identifier =
            Self::normalize(&source, version).ok_or_else(|| Error::InvalidJavaIdentifier {
                identifier: source.clone(),
            })?;
        let restricted_since = match identifier.as_str() {
            "_" => Some(JavaVersion::JAVA_9),
            _ => None,
        };
        match !Self::reserved(&identifier)
            && restricted_since.is_none_or(|minimum| version < minimum)
        {
            true => Ok(Self {
                spelling: identifier,
                restricted_since,
            }),
            false => Err(Error::InvalidJavaIdentifier { identifier: source }),
        }
    }

    pub fn escape_for(identifier: impl Into<String>, version: JavaVersion) -> Result<Self> {
        version.validate()?;
        let source = identifier.into();
        let identifier = Self::normalize(&source, version)
            .ok_or(Error::InvalidJavaIdentifier { identifier: source })?;
        match Self::reserved(&identifier) || (identifier == "_" && version >= JavaVersion::JAVA_9) {
            true => Self::parse_for(format!("_{identifier}"), version),
            false => Self::parse_for(identifier, version),
        }
    }

    pub fn known(identifier: &'static str) -> Self {
        Self::parse(identifier).expect("static Java identifier must be valid")
    }

    pub fn validate(&self, version: JavaVersion) -> Result<()> {
        version.validate()?;
        match Self::normalize(&self.spelling, version).as_deref() != Some(self.spelling.as_str())
            || self
                .restricted_since
                .is_some_and(|minimum| version >= minimum)
        {
            true => Err(Error::InvalidJavaIdentifier {
                identifier: self.to_string(),
            }),
            false => Ok(()),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.spelling
    }

    fn normalize(identifier: &str, version: JavaVersion) -> Option<String> {
        let identifiers = JavaIdentifiers::for_version(version);
        let mut characters = identifier.chars();
        let first = characters.next()?;
        identifiers.start(first).then(|| {
            characters.try_fold(first.to_string(), |mut normalized, character| {
                identifiers.part(character).then(|| {
                    if !identifiers.ignorable(character) {
                        normalized.push(character);
                    }
                    normalized
                })
            })
        })?
    }

    fn reserved(identifier: &str) -> bool {
        Syntax::keyword(identifier) || matches!(identifier, "false" | "null" | "true")
    }
}

impl TypeIdentifier {
    pub fn parse(identifier: impl Into<String>, version: JavaVersion) -> Result<Self> {
        let identifier = Identifier::parse_for(identifier, version)?;
        let restricted_since = Self::restriction(identifier.as_str());
        match restricted_since.is_some_and(|minimum| version >= minimum) {
            true => Err(Error::InvalidJavaIdentifier {
                identifier: identifier.to_string(),
            }),
            false => Ok(Self {
                identifier,
                restricted_since,
            }),
        }
    }

    pub fn escape(identifier: impl Into<String>, version: JavaVersion) -> Result<Self> {
        let identifier = Identifier::escape_for(identifier, version)?;
        match Self::restriction(identifier.as_str()).is_some_and(|minimum| version >= minimum) {
            true => Self::parse(format!("_{identifier}"), version),
            false => Ok(Self {
                restricted_since: Self::restriction(identifier.as_str()),
                identifier,
            }),
        }
    }

    pub fn known(identifier: &'static str, version: JavaVersion) -> Self {
        Self::parse(identifier, version).expect("static Java type identifier must be valid")
    }

    pub fn validate(&self, version: JavaVersion) -> Result<()> {
        self.identifier.validate(version)?;
        match self
            .restricted_since
            .is_some_and(|minimum| version >= minimum)
        {
            true => Err(Error::InvalidJavaIdentifier {
                identifier: self.to_string(),
            }),
            false => Ok(()),
        }
    }

    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }

    pub fn as_str(&self) -> &str {
        self.identifier.as_str()
    }

    fn restriction(identifier: &str) -> Option<JavaVersion> {
        match identifier {
            "var" => JavaVersion::new(10),
            "yield" => JavaVersion::new(14),
            "record" => Some(JavaVersion::JAVA_16),
            "sealed" | "permits" => Some(JavaVersion::JAVA_17),
            _ => None,
        }
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.spelling)
    }
}

impl fmt::Display for TypeIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identifier.fmt(formatter)
    }
}

impl sealed::SyntaxFragment for Identifier {}

#[cfg(test)]
mod tests {
    use crate::target::java::JavaVersion;

    use super::{Identifier, TypeIdentifier};

    #[test]
    fn distinguishes_reserved_and_contextual_identifiers() {
        assert!(Identifier::parse("two-words").is_err());
        assert!(Identifier::parse("9lives").is_err());
        assert_eq!(
            Identifier::escape_for("class", JavaVersion::JAVA_8)
                .unwrap()
                .as_str(),
            "_class"
        );
        assert_eq!(
            Identifier::escape_for("true", JavaVersion::JAVA_8)
                .unwrap()
                .as_str(),
            "_true"
        );
        assert_eq!(
            Identifier::parse_for("_", JavaVersion::JAVA_8)
                .unwrap()
                .as_str(),
            "_"
        );
        assert!(Identifier::parse_for("_", JavaVersion::JAVA_9).is_err());
        assert_eq!(
            Identifier::escape_for("_", JavaVersion::JAVA_9)
                .unwrap()
                .as_str(),
            "__"
        );
        ["module", "record", "sealed", "permits", "yield", "var"]
            .into_iter()
            .try_for_each(|identifier| Identifier::parse(identifier).map(drop))
            .unwrap();
    }

    #[test]
    fn validates_unicode_identifiers() {
        assert_eq!(Identifier::parse("café").unwrap().as_str(), "café");
        assert_eq!(Identifier::parse("東京").unwrap().as_str(), "東京");
        assert_eq!(Identifier::parse("𐐀value").unwrap().as_str(), "𐐀value");
        assert_eq!(Identifier::parse("€value").unwrap().as_str(), "€value");
        assert_eq!(Identifier::parse("‿value").unwrap().as_str(), "‿value");
        assert_eq!(Identifier::parse("a\u{301}").unwrap().as_str(), "a\u{301}");
        assert_eq!(
            Identifier::parse("value\u{200c}").unwrap().as_str(),
            "value"
        );
        assert_eq!(
            Identifier::parse("\u{1885}value").unwrap().as_str(),
            "\u{1885}value"
        );
        assert!(Identifier::parse("\u{19b0}value").is_err());
        assert!(Identifier::parse("value\u{180e}").is_err());
        assert!(Identifier::parse("𞤀value").is_err());
        assert!(Identifier::parse("😀value").is_err());
    }

    #[test]
    fn validates_unicode_identifiers_for_the_selected_release() {
        assert!(Identifier::parse_for("\u{1885}value", JavaVersion::JAVA_8).is_ok());
        assert!(Identifier::parse_for("\u{1885}value", JavaVersion::JAVA_17).is_err());
        assert!(Identifier::parse_for("𞤀value", JavaVersion::JAVA_9).is_err());
        assert!(Identifier::parse_for("𞤀value", JavaVersion::JAVA_11).is_ok());
        assert!(Identifier::parse_for("value", JavaVersion(7)).is_err());
        assert!(Identifier::parse_for("value", JavaVersion(27)).is_err());
    }

    #[test]
    fn applies_release_sensitive_type_identifier_restrictions() {
        assert!(TypeIdentifier::parse("var", JavaVersion::JAVA_9).is_ok());
        assert!(TypeIdentifier::parse("var", JavaVersion::new(10).unwrap()).is_err());
        assert!(TypeIdentifier::parse("yield", JavaVersion::new(13).unwrap()).is_ok());
        assert!(TypeIdentifier::parse("yield", JavaVersion::new(14).unwrap()).is_err());
        assert!(TypeIdentifier::parse("record", JavaVersion::new(15).unwrap()).is_ok());
        assert!(TypeIdentifier::parse("record", JavaVersion::JAVA_16).is_err());
        assert!(TypeIdentifier::parse("sealed", JavaVersion::JAVA_16).is_ok());
        assert!(TypeIdentifier::parse("sealed", JavaVersion::JAVA_17).is_err());
        assert!(TypeIdentifier::parse("module", JavaVersion::JAVA_24).is_ok());
        assert_eq!(
            TypeIdentifier::escape("class", JavaVersion::JAVA_24)
                .unwrap()
                .as_str(),
            "_class"
        );
    }
}
