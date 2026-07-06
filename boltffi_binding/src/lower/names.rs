use boltffi_ast::{FieldDef as SourceField, SourceName};

use crate::{CanonicalName, FieldKey, NamePart};

impl From<&SourceName> for CanonicalName {
    fn from(name: &SourceName) -> Self {
        Self::from_source(
            name.spelling(),
            name.parts()
                .map(|part| NamePart::new(part.as_str()))
                .collect(),
        )
    }
}

impl From<&SourceField> for FieldKey {
    fn from(field: &SourceField) -> Self {
        Self::Named(CanonicalName::from(&field.name))
    }
}
