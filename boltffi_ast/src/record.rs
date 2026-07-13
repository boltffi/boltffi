use serde::{Deserialize, Serialize};

use crate::TypeExpr;
use crate::{
    DefaultValue, DeprecationInfo, DocComment, RecordId, ReprAttr, Source, SourceName, SourceSpan,
    UserAttr,
};

/// How an exported record is encoded across the binding boundary.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RecordEncoding {
    /// The normal Bolt record encoding, decoded into host-owned fields.
    #[default]
    Standard,
    /// The record is boxed as an opaque native handle; host accesses fields
    /// through generated per-field Rust accessor exports.
    NativeOpaque,
}

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
    /// Requested encoded-record storage behavior.
    #[serde(default)]
    pub encoding: RecordEncoding,
    /// User attributes preserved from the struct.
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the record.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the record.
    pub deprecated: Option<DeprecationInfo>,
    /// Methods attached to the record.
    pub methods: Vec<crate::MethodDef>,
    /// Visibility and source location for diagnostics.
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
            encoding: RecordEncoding::Standard,
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
    pub doc: Option<DocComment>,
    /// Default value written for the field.
    pub default: Option<DefaultValue>,
    /// User attributes preserved from the field.
    pub user_attrs: Vec<UserAttr>,
    /// Visibility and source location for diagnostics.
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
