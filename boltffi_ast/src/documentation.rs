use serde::{Deserialize, Serialize};

/// Documentation collected from Rust doc comments.
///
/// The text is the body of the Rust documentation after the leading doc markers
/// have been removed. It stays as text in the Rustdoc-flavored Markdown format
/// the author wrote.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DocComment(String);

impl DocComment {
    /// Builds documentation from scanner-collected text.
    ///
    /// The `text` parameter should be the doc comment body after Rust has
    /// stripped the leading doc markers. The text is stored unchanged.
    ///
    /// Returns a doc comment value suitable for attaching to declarations,
    /// fields, variants, parameters, and callables.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Returns the documentation body.
    ///
    /// The returned value is the documentation body.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Deprecation metadata collected from Rust attributes.
///
/// This mirrors the useful parts of Rust's `deprecated` attribute: an optional
/// human note and an optional version string.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DeprecationInfo {
    /// Optional message supplied by the Rust author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Optional version string supplied by the Rust author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

impl DeprecationInfo {
    /// Builds deprecation metadata from optional attribute parts.
    ///
    /// The `note` parameter carries the human-facing deprecation reason. The
    /// `since` parameter carries the version string when the attribute included
    /// one.
    ///
    /// Returns deprecation metadata with the note and version string preserved.
    pub fn new(note: Option<String>, since: Option<String>) -> Self {
        Self { note, since }
    }
}
