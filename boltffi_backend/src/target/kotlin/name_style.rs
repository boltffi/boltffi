use std::fmt;
use std::path::PathBuf;

use boltffi_binding::{CanonicalName, NamePart};

use crate::{
    bridge::jni::JvmClassPath,
    core::{Error, Result, name_case},
    target::kotlin::{
        KotlinHost,
        syntax::{Identifier, TypeName},
    },
};

/// A Kotlin package name backed by the JVM package grammar.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KotlinPackage {
    name: String,
}

/// A Kotlin source file stem.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KotlinFile {
    name: String,
}

/// A native library name accepted by Kotlin's runtime loader.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KotlinLibrary {
    name: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name {
    source: CanonicalName,
}

impl KotlinPackage {
    /// Parses a Kotlin package name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        JvmClassPath::new(name.clone(), "Native")?;
        Ok(Self { name })
    }

    /// Returns the package name text.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Returns the package directory path.
    pub fn directory(&self) -> PathBuf {
        self.name.split('.').collect()
    }
}

impl fmt::Display for KotlinPackage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl KotlinFile {
    /// Parses a Kotlin source file stem.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if Self::valid(&name) {
            Ok(Self { name })
        } else {
            Err(Error::InvalidKotlinIdentifier { identifier: name })
        }
    }

    /// Returns the file stem.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Returns the generated source path for this file inside a package.
    pub fn path(&self, package: &KotlinPackage) -> PathBuf {
        package.directory().join(format!("{}.kt", self.name))
    }

    fn valid(name: &str) -> bool {
        let mut characters = name.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl fmt::Display for KotlinFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl KotlinLibrary {
    /// Parses a Kotlin native library load name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if !name.is_empty()
            && !name
                .chars()
                .any(|character| matches!(character, '/' | '\\'))
        {
            Ok(Self { name })
        } else {
            Err(KotlinHost::unsupported("kotlin native library name"))
        }
    }

    /// Returns the library name passed to Kotlin's runtime loader.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for KotlinLibrary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Name {
    pub fn new(name: &CanonicalName) -> Self {
        Self {
            source: name.clone(),
        }
    }

    pub fn function(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn parameter(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn type_name(&self) -> TypeName {
        TypeName::new(self.upper_camel())
    }

    pub fn variant(&self) -> Result<Identifier> {
        Identifier::escape(self.upper_camel())
    }

    pub fn enum_entry(&self) -> Result<Identifier> {
        Identifier::escape(self.screaming_snake())
    }

    pub fn generated(&self, suffix: &str) -> Result<Identifier> {
        Identifier::parse(format!("__boltffi_{}_{}", self.lower_camel(), suffix))
    }

    fn lower_camel(&self) -> String {
        name_case::lower_camel(&self.source)
    }

    fn upper_camel(&self) -> String {
        name_case::upper_camel(&self.source)
    }

    fn screaming_snake(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .map(str::to_ascii_uppercase)
            .collect::<Vec<_>>()
            .join("_")
    }
}
