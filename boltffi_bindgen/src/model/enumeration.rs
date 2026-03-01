use super::{record::RecordField, types::Deprecation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enumeration {
    pub name: String,
    pub variants: Vec<Variant>,
    pub doc: Option<String>,
    pub deprecated: Option<Deprecation>,
    pub is_error: bool,
}

impl Enumeration {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            variants: Vec::new(),
            doc: None,
            deprecated: None,
            is_error: false,
        }
    }

    pub fn as_error(mut self) -> Self {
        self.is_error = true;
        self
    }

    pub fn with_variant(mut self, variant: Variant) -> Self {
        self.variants.push(variant);
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn maybe_doc(self, doc: Option<String>) -> Self {
        match doc {
            Some(d) => self.with_doc(d),
            None => self,
        }
    }

    pub fn with_deprecated(mut self, deprecation: Deprecation) -> Self {
        self.deprecated = Some(deprecation);
        self
    }

    pub fn is_c_style(&self) -> bool {
        self.variants.iter().all(Variant::is_unit)
    }

    pub fn is_data_enum(&self) -> bool {
        !self.is_c_style()
    }

    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,
    pub discriminant: Option<i64>,
    pub fields: Vec<RecordField>,
    pub doc: Option<String>,
}

impl Variant {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            discriminant: None,
            fields: Vec::new(),
            doc: None,
        }
    }

    pub fn with_discriminant(mut self, value: i64) -> Self {
        self.discriminant = Some(value);
        self
    }

    pub fn with_field(mut self, field: RecordField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.doc = Some(doc.into());
        self
    }

    pub fn maybe_doc(self, doc: Option<String>) -> Self {
        match doc {
            Some(d) => self.with_doc(d),
            None => self,
        }
    }

    pub fn is_unit(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn has_fields(&self) -> bool {
        !self.fields.is_empty()
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }
}
