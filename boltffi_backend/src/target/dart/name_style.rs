use boltffi_binding::{CanonicalName, NamePart};

use crate::core::{Error, Result};

use super::syntax::Identifier;

pub struct Name<'name> {
    source: &'name CanonicalName,
}

impl<'name> Name<'name> {
    pub fn new(source: &'name CanonicalName) -> Self {
        Self { source }
    }

    pub fn upper_camel(&self) -> Result<Identifier> {
        Identifier::parse(
            self.source
                .parts()
                .iter()
                .map(NamePart::as_str)
                .map(Self::capitalized)
                .collect::<String>(),
        )
    }

    pub fn lower_camel(&self) -> Result<Identifier> {
        let mut parts = self.source.parts().iter();
        let first =
            parts
                .next()
                .map(NamePart::as_str)
                .ok_or_else(|| Error::InvalidDartIdentifier {
                    identifier: String::new(),
                })?;
        Identifier::normalize(
            std::iter::once(first.to_owned())
                .chain(parts.map(NamePart::as_str).map(Self::capitalized))
                .collect::<String>(),
        )
    }

    pub fn snake(&self) -> String {
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

pub(crate) fn cancellation_token_name<'a>(names: impl Iterator<Item = &'a str>) -> &'static str {
    let mut primary_used = false;
    let mut secondary_used = false;
    for name in names {
        primary_used |= name == "cancellationToken";
        secondary_used |= name == "boltCancellationToken";
    }
    if !primary_used {
        "cancellationToken"
    } else if !secondary_used {
        "boltCancellationToken"
    } else {
        "boltCancellationToken$"
    }
}

#[cfg(test)]
mod tests {
    use boltffi_binding::NamePart;

    use super::*;

    #[test]
    fn canonical_names_follow_dart_conventions() {
        let source = CanonicalName::new(vec![NamePart::new("http"), NamePart::new("client")]);
        let name = Name::new(&source);

        assert_eq!(name.upper_camel().unwrap().as_str(), "HttpClient");
        assert_eq!(name.lower_camel().unwrap().as_str(), "httpClient");
        assert_eq!(name.snake(), "http_client");
    }
}
