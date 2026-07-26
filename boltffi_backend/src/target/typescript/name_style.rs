use std::{fmt, path::PathBuf};

use boltffi_binding::CanonicalName;

use crate::core::{Result, name_case};

use super::syntax::{Identifier, MemberName, TypeName};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModuleName(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name(CanonicalName);

impl ModuleName {
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        Identifier::parse(name.clone())?;
        Ok(Self(name))
    }

    pub fn browser_path(&self) -> PathBuf {
        PathBuf::from(format!("{}.ts", self.0))
    }

    pub fn node_path(&self) -> PathBuf {
        PathBuf::from(format!("{}_node.ts", self.0))
    }

    pub fn wasm_file(&self) -> String {
        format!("{}_bg.wasm", self.0)
    }
}

impl fmt::Display for ModuleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Name {
    pub fn new(name: &CanonicalName) -> Self {
        Self(name.clone())
    }

    pub fn identifier(&self) -> Result<Identifier> {
        Identifier::escape(name_case::lower_camel(&self.0))
    }

    pub fn type_name(&self) -> TypeName {
        TypeName::named(name_case::upper_camel(&self.0))
    }

    pub fn member(&self) -> Result<MemberName> {
        MemberName::parse(name_case::lower_camel(&self.0))
    }

    pub fn codec_identifier(&self) -> Result<Identifier> {
        Identifier::parse(format!("{}Codec", name_case::upper_camel(&self.0)))
    }

    pub fn variant_identifier(&self) -> Result<Identifier> {
        Identifier::escape(name_case::upper_camel(&self.0))
    }
}
