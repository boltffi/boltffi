use std::{fmt, path::PathBuf};

use boltffi_binding::CanonicalName;

use crate::{
    core::{Error, Result, lexical::NameStem, name_case},
    target::swift::syntax::{Identifier, TypeName},
};

/// A Swift module name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SwiftModule {
    name: String,
}

/// A Swift source file name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SwiftFile {
    name: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name {
    source: CanonicalName,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GeneratedLocal {
    ReturnBuffer,
    ReturnHandle,
    ErrorBuffer,
    WireReader,
    ErrorReader,
    FutureHandle,
    FutureStatus,
    ClosureStatus,
    ClosureInvoke,
    StreamSubscription,
    StreamBatch,
    StreamBatchCount,
}

impl SwiftModule {
    /// Parses a Swift module name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if SwiftFile::valid(&name) {
            Ok(Self { name })
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier: name })
        }
    }

    /// Returns the module name text.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for SwiftModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl SwiftFile {
    /// Parses a Swift source file name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if Self::valid(&name) {
            Ok(Self { name })
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier: name })
        }
    }

    /// Creates the default source file for a module.
    pub fn from_module(module: &SwiftModule) -> Self {
        Self {
            name: module.as_str().to_owned(),
        }
    }

    /// Returns the generated source path.
    pub fn path(&self) -> PathBuf {
        PathBuf::from(format!("{}.swift", self.name))
    }

    fn valid(name: &str) -> bool {
        let mut characters = name.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl fmt::Display for SwiftFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
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

    pub fn field(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn variant(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn generated(&self, suffix: &str) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi{}{}",
            self.upper_camel(),
            name_case::upper_camel_from_snake(suffix)
        ))
    }

    pub fn type_name(&self) -> TypeName {
        TypeName::new(self.upper_camel())
    }

    fn lower_camel(&self) -> String {
        name_case::lower_camel(&self.source)
    }

    fn upper_camel(&self) -> String {
        name_case::upper_camel(&self.source)
    }
}

impl GeneratedLocal {
    pub fn stem(self) -> NameStem {
        NameStem::new(self.role())
    }

    pub fn suffixed_stem(self, suffix: &str) -> NameStem {
        self.stem().suffixed(suffix)
    }

    pub fn identifier(self) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi{}",
            name_case::upper_camel_from_snake(self.role())
        ))
    }

    pub fn suffixed(self, suffix: &str) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi{}{}",
            name_case::upper_camel_from_snake(self.role()),
            name_case::upper_camel_from_snake(suffix)
        ))
    }

    fn role(self) -> &'static str {
        match self {
            Self::ReturnBuffer => "result",
            Self::ReturnHandle => "handle",
            Self::ErrorBuffer => "error",
            Self::WireReader => "reader",
            Self::ErrorReader => "error_reader",
            Self::FutureHandle => "future",
            Self::FutureStatus => "status",
            Self::ClosureStatus => "closure_status",
            Self::ClosureInvoke => "closure_invoke",
            Self::StreamSubscription => "subscription",
            Self::StreamBatch => "stream_batch",
            Self::StreamBatchCount => "stream_count",
        }
    }
}
