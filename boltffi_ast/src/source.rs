use serde::{Deserialize, Serialize};

/// A Rust source file that contributed to the source contract.
///
/// The path is stored as scanner-provided text, which works for absolute paths,
/// workspace-relative paths, and fixture paths.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SourceFile(String);

impl SourceFile {
    /// Builds a source file reference.
    ///
    /// The `path` parameter is stored exactly as the scanner reported it.
    ///
    /// Returns a source file value.
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Returns the stored source path.
    ///
    /// The returned path is the same text passed to [`SourceFile::new`].
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A byte span in a Rust source file.
///
/// Spans use byte offsets, the common unit shared by parsers, diagnostics, and
/// source maps.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Source file that owns the byte range.
    pub file: SourceFile,
    /// Inclusive byte offset where the span starts.
    pub start: usize,
    /// Exclusive byte offset where the span ends.
    pub end: usize,
}

impl SourceSpan {
    /// Builds a source span from a file and byte range.
    ///
    /// The `file` parameter identifies the source file. The `start` and `end`
    /// parameters are byte offsets in that file, with `end` excluded.
    ///
    /// Returns a source span with the provided byte offsets.
    pub fn new(file: SourceFile, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }
}

/// The source visibility written on an exported Rust item.
///
/// Visibility lets diagnostics say whether the original Rust item was private,
/// public, or restricted to a smaller scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Visibility {
    /// A private item.
    Private,
    /// A public item written as `pub`.
    Public,
    /// A restricted public item such as `pub(crate)` or `pub(super)`.
    Restricted(String),
}

/// Common source metadata shared by AST nodes.
///
/// Declarations and members embed this value when they need both Rust
/// visibility and optional span information.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Visibility written on the source item.
    pub visibility: Visibility,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub span: Option<SourceSpan>,
}

impl Default for Source {
    fn default() -> Self {
        Self::exported()
    }
}

impl Source {
    /// Creates source metadata for an exported AST node.
    ///
    /// Use this when a builder helper is constructing an exported declaration
    /// before the scanner has attached the exact Rust span. The scanner can
    /// still replace the value with `pub(crate)`, `pub(super)`, or any other
    /// source visibility it actually saw.
    ///
    /// Returns public source metadata without a span.
    pub fn exported() -> Self {
        Self {
            visibility: Visibility::Public,
            span: None,
        }
    }

    /// Whether the visibility is public, the default that serialized metadata omits.
    pub fn is_exported(&self) -> bool {
        self.visibility == Visibility::Public
    }

    /// Builds source metadata from visibility and an optional span.
    ///
    /// The `visibility` parameter records the source visibility. The `span`
    /// parameter is kept only in macro memory and is stripped from serialized
    /// metadata.
    ///
    /// Returns source metadata that can be attached to a declaration or member.
    pub fn new(visibility: Visibility, span: Option<SourceSpan>) -> Self {
        Self { visibility, span }
    }
}
