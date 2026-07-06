use boltffi_binding::{CanonicalName, FieldKey};

use crate::core::{Error, Result, name_case};

#[derive(Clone, Debug)]
pub struct Spelling {
    name: CanonicalName,
}

impl Spelling {
    pub fn new(name: &CanonicalName) -> Self {
        Self { name: name.clone() }
    }

    pub fn parameter(self) -> String {
        self.join("_", str::to_owned)
    }

    pub fn typedef(self) -> String {
        format!("___{}", name_case::upper_camel(&self.name))
    }

    pub fn constant(self) -> String {
        self.join("_", str::to_ascii_uppercase)
    }

    fn join(self, separator: &str, transform: impl Fn(&str) -> String) -> String {
        self.name
            .parts()
            .iter()
            .map(|part| transform(part.as_str()))
            .collect::<Vec<_>>()
            .join(separator)
    }
}

pub struct Field(FieldKey);

impl Field {
    pub fn new(field: &FieldKey) -> Self {
        Self(field.clone())
    }

    pub fn spelling(&self) -> Result<String> {
        match &self.0 {
            FieldKey::Named(name) => Ok(Spelling::new(name).parameter()),
            FieldKey::Position(position) => Ok(format!("field_{position}")),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown field key",
            }),
        }
    }
}

pub struct EnumConstant {
    owner: CanonicalName,
    variant: CanonicalName,
}

impl EnumConstant {
    pub fn new(owner: &CanonicalName, variant: &CanonicalName) -> Self {
        Self {
            owner: owner.clone(),
            variant: variant.clone(),
        }
    }

    pub fn spelling(&self) -> String {
        [
            Spelling::new(&self.owner).constant(),
            Spelling::new(&self.variant).constant(),
        ]
        .join("_")
    }
}
