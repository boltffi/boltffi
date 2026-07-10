use std::fmt;

use boltffi_binding::{CanonicalName, NamePart};

use crate::core::{Error, Result};

use super::syntax::Identifier;

pub(crate) struct Name<'name> {
    source: &'name CanonicalName,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Namespace(Vec<Identifier>);

impl<'name> Name<'name> {
    pub(crate) fn new(source: &'name CanonicalName) -> Self {
        Self { source }
    }

    pub(crate) fn pascal(&self) -> Result<Identifier> {
        Identifier::parse(
            self.source
                .parts()
                .iter()
                .map(NamePart::as_str)
                .map(Self::capitalized)
                .collect::<String>(),
        )
    }

    pub(crate) fn camel(&self) -> Result<Identifier> {
        let mut parts = self.source.parts().iter();
        let first =
            parts
                .next()
                .map(NamePart::as_str)
                .ok_or_else(|| Error::InvalidCSharpIdentifier {
                    identifier: String::new(),
                })?;
        let name = std::iter::once(first.to_owned())
            .chain(parts.map(NamePart::as_str).map(Self::capitalized))
            .collect::<String>();
        Identifier::escape(name)
    }

    pub(crate) fn snake(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_")
    }

    fn capitalized(part: &str) -> String {
        let mut characters = part.chars();
        characters.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(characters).collect()
        })
    }
}

impl Namespace {
    pub(crate) fn parse(namespace: &str) -> Result<Self> {
        if namespace.is_empty() {
            return Err(Error::InvalidCSharpNamespace {
                namespace: namespace.to_owned(),
            });
        }
        namespace
            .split('.')
            .map(Identifier::escape)
            .collect::<Result<Vec<_>>>()
            .map(Self)
            .map_err(|_| Error::InvalidCSharpNamespace {
                namespace: namespace.to_owned(),
            })
    }

    pub(crate) fn from_canonical(name: &CanonicalName) -> Result<Self> {
        Ok(Self(vec![Name::new(name).pascal()?]))
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            &self
                .0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
        )
    }
}

#[cfg(test)]
mod tests {
    use boltffi_binding::NamePart;

    use super::*;

    #[test]
    fn canonical_names_follow_csharp_conventions() {
        let source = CanonicalName::new(vec![NamePart::new("http"), NamePart::new("client")]);
        let name = Name::new(&source);

        assert_eq!(name.pascal().unwrap().as_str(), "HttpClient");
        assert_eq!(name.camel().unwrap().as_str(), "httpClient");
        assert_eq!(name.snake(), "http_client");
    }

    #[test]
    fn namespace_escapes_keyword_segments() {
        assert_eq!(
            Namespace::parse("Company.event").unwrap().to_string(),
            "Company.@event"
        );
    }
}
