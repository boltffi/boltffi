use serde::{Deserialize, Serialize};

use crate::TypeExpr;
use crate::{
    DefaultValue, DeprecationInfo, DocComment, RecordId, ReprAttr, Source, SourceName, SourceSpan,
    UserAttr,
};

/// A Rust struct exported as a BoltFFI record.
///
/// A record keeps the struct-shaped API a binding author sees: fields,
/// representation hints, methods, attributes, documentation, and source
/// location. Layout-specific data is supplied by the resolved contract that
/// follows this source model.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RecordDef {
    /// Stable record identity derived from the canonical Rust path.
    pub id: RecordId,
    /// Source record name.
    pub name: SourceName,
    /// Fields written on the Rust struct.
    pub fields: Vec<FieldDef>,
    /// `repr` attributes written on the struct.
    pub repr: ReprAttr,
    /// User attributes preserved from the struct.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<DeprecationInfo>,
    /// Methods attached to the record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<crate::MethodDef>,
    /// Visibility and source location for diagnostics.
    #[serde(default, skip_serializing_if = "Source::is_exported")]
    pub source: Source,
    /// Proc-macro span available while scanning the user crate.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl RecordDef {
    /// Builds an empty record definition.
    ///
    /// The `id` parameter is the stable record ID. The `name` parameter is the
    /// canonical source name.
    ///
    /// Returns a record with no fields, attributes, or methods.
    pub fn new(id: RecordId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            fields: Vec::new(),
            repr: ReprAttr::none(),
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            methods: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }
}

/// A named field written inside a record or a struct-style enum variant.
///
/// Fields carry the API name, the source type expression, optional
/// documentation, an optional default, and any attributes written directly on
/// the field.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    /// Source field name.
    pub name: SourceName,
    /// Rust source type written for the field.
    pub type_expr: TypeExpr,
    /// Documentation attached to the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocComment>,
    /// Default value written for the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    /// User attributes preserved from the field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_attrs: Vec<UserAttr>,
    /// Visibility and source location for diagnostics.
    #[serde(default, skip_serializing_if = "Source::is_exported")]
    pub source: Source,
    /// Proc-macro span available while scanning the user crate.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl FieldDef {
    /// Builds a field without documentation, attributes, or default value.
    ///
    /// The `name` parameter is the canonical field name. The `rust_type`
    /// parameter is the field's source type.
    ///
    /// Returns a field definition that can be attached to records and variants.
    pub fn new(name: impl Into<SourceName>, type_expr: TypeExpr) -> Self {
        Self {
            name: name.into(),
            type_expr,
            doc: None,
            default: None,
            user_attrs: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{CanonicalName, Visibility};

    #[test]
    fn serialized_field_omits_what_it_does_not_carry() {
        let field = FieldDef::new(
            SourceName::new("width", CanonicalName::single("width")),
            TypeExpr::String,
        );
        let serialized = serde_json::to_value(&field).expect("field serializes");
        assert_eq!(
            serialized,
            json!({ "name": { "spelling": "width" }, "type_expr": "String" }),
            "absent documentation, default, attributes and public visibility are left out"
        );
        assert_eq!(
            serde_json::from_value::<FieldDef>(serialized).expect("field deserializes"),
            field,
            "the omitted parts read back as written"
        );
    }

    #[test]
    fn serialized_field_keeps_a_visibility_that_is_not_public() {
        let mut field = FieldDef::new(
            SourceName::new("width", CanonicalName::single("width")),
            TypeExpr::String,
        );
        field.source = Source::new(Visibility::Private, None);
        let serialized = serde_json::to_value(&field).expect("field serializes");
        assert_eq!(
            serialized["source"],
            json!({ "visibility": "Private" }),
            "a private field says so"
        );
        assert_eq!(
            serde_json::from_value::<FieldDef>(serialized).expect("field deserializes"),
            field,
            "the visibility is read back"
        );
    }
}
