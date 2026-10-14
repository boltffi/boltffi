use serde::{Deserialize, Serialize};

use crate::{
    ClassId, DeprecationInfo, DocComment, Source, SourceName, SourceSpan, StreamId, TypeExpr,
    UserAttr,
};

/// A stream-producing export in the source contract.
///
/// Streams describe an exported Rust operation that yields values over time.
/// The item type says what each yield produces; the mode says how the author
/// wants the stream surfaced conceptually.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct StreamDef {
    /// Stable stream identity derived from the canonical Rust path.
    pub id: StreamId,
    /// Source stream name.
    pub name: SourceName,
    /// Class owner when the stream is attached to a class.
    pub owner: Option<ClassId>,
    /// Rust source item type yielded by the stream.
    pub item_type: TypeExpr,
    /// Rust source error type the stream may end with, when it is fallible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<TypeExpr>,
    /// Source stream mode requested by the author.
    pub mode: StreamMode,
    /// User attributes preserved from the stream declaration.
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the stream.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the stream.
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl StreamDef {
    /// Builds a stream definition.
    ///
    /// The `id` parameter is the stable stream ID. The `name` parameter is the
    /// canonical stream name. The `item_type` parameter is the yielded source
    /// type.
    ///
    /// Returns an async stream with no owner, attributes, or documentation.
    pub fn new(id: StreamId, name: impl Into<SourceName>, item_type: TypeExpr) -> Self {
        Self {
            id,
            name: name.into(),
            owner: None,
            item_type,
            error_type: None,
            mode: StreamMode::Async,
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }
}

/// The source mode requested for a stream.
///
/// The modes are intentionally high level: async iteration, batched reads, or
/// callback delivery.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum StreamMode {
    /// Values are produced through an asynchronous stream.
    #[default]
    Async,
    /// Values are produced in batches.
    Batch,
    /// Values are delivered through callbacks.
    Callback,
}
