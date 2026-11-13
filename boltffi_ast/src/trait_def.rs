use serde::{Deserialize, Serialize};

use crate::{
    DeprecationInfo, DocComment, MethodDef, Source, SourceName, SourceSpan, TraitId, UserAttr,
};

/// A trait the source crate exports through BoltFFI.
///
/// Represents a Rust trait whose methods can be implemented outside Rust;
/// at the FFI boundary the trait plays a callback role (foreign code
/// provides the impl, Rust calls into it). The source entity is a trait,
/// so the type is named for the entity, not for the boundary role.
///
/// Inline closure parameters carry an [`FnTrait`](crate::FnTrait) bound
/// instead of resolving to a trait declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TraitDef {
    /// Stable trait identity derived from the canonical Rust path.
    pub id: TraitId,
    /// Source trait name.
    pub name: SourceName,
    /// Methods that an implementer must provide.
    pub methods: Vec<MethodDef>,
    /// User attributes preserved from the trait declaration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the trait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the trait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    #[serde(default, skip_serializing_if = "Source::is_exported")]
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl TraitDef {
    /// Builds an empty trait definition.
    ///
    /// The `id` parameter is the stable trait ID. The `name` parameter is
    /// the canonical trait name.
    ///
    /// Returns a trait definition with no methods or attributes.
    pub fn new(id: TraitId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            methods: Vec::new(),
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }
}
