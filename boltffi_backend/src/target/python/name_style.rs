use boltffi_binding::{CanonicalName, NamePart};

use crate::core::{Error, Result, name_case};

use super::syntax::Identifier;

pub struct Name {
    source: CanonicalName,
}

/// A Python package module name accepted by generated Python bindings.
///
/// The value is guaranteed to be a Python identifier and not a Python keyword.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageModule {
    name: String,
}

impl Name {
    pub fn new(source: &CanonicalName) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn function(&self) -> Result<Identifier> {
        let name = self
            .source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_");
        Identifier::escape(name)
    }

    pub fn function_text(&self) -> Result<String> {
        self.function().map(|identifier| identifier.to_string())
    }

    pub fn class(&self) -> String {
        name_case::upper_camel(&self.source)
    }

    pub fn enum_member(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .map(str::to_ascii_uppercase)
            .collect::<Vec<_>>()
            .join("_")
    }

    pub fn position_field(position: u32) -> Result<Identifier> {
        Identifier::parse(format!("field_{position}"))
    }
}

impl PackageModule {
    /// Parses a configured Python package module name.
    ///
    /// Returns an error when the name is empty, is not a Python identifier, or is a Python keyword.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if Identifier::parse(name.clone()).is_ok() {
            Ok(Self { name })
        } else {
            Err(Error::InvalidPythonPackageModule { name })
        }
    }

    /// Creates a package module name from a canonical BoltFFI package name.
    pub fn from_canonical(name: &CanonicalName) -> Result<Self> {
        Ok(Self {
            name: Name::new(name).function_text()?,
        })
    }

    /// Returns the Python module name.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}
