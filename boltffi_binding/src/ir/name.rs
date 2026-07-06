use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
};

/// One word inside a [`CanonicalName`].
///
/// Names in the binding contract are stored as ordered segments rather than
/// as a pre-cased string so each target language can apply its own casing
/// rule. A `NamePart` is one of those segments, normalized to the form the
/// classifier produced.
///
/// # Example
///
/// The Rust type `UserProfile` becomes two parts: `["user", "profile"]`. A
/// PascalCase target joins them as `UserProfile`; a snake_case target as
/// `user_profile`; a SCREAMING_SNAKE target as `USER_PROFILE`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamePart(String);

impl NamePart {
    /// Stores one already-normalized segment.
    pub fn new(part: impl Into<String>) -> Self {
        Self(part.into())
    }

    /// Returns the segment.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NamePart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for NamePart {
    fn from(part: &str) -> Self {
        Self::new(part)
    }
}

impl From<String> for NamePart {
    fn from(part: String) -> Self {
        Self::new(part)
    }
}

/// A name in the binding contract before any target language has spelled it.
///
/// Storing names as ordered segments is the only way to render the same
/// name as `Point` in Swift, `point` in Python, and `point_t` in C without
/// re-parsing the original Rust identifier in each target.
///
/// Empty names are accepted by the constructor and rejected during
/// validation, so a deserialized contract can still produce a precise
/// diagnostic for the offending declaration instead of failing to load.
///
/// # Example
///
/// `CanonicalName::single("status")` is a one-segment name. For the Rust
/// type `XmlParser`, the segments are `["xml", "parser"]`; the Swift
/// renderer produces `XmlParser`, the snake_case renderer produces
/// `xml_parser`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CanonicalName {
    parts: Vec<NamePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_spelling: Option<Box<str>>,
}

impl CanonicalName {
    /// Builds a name from already-normalized parts.
    pub fn new(parts: Vec<NamePart>) -> Self {
        Self {
            parts,
            source_spelling: None,
        }
    }

    /// Builds a name from normalized parts and the Rust source spelling.
    pub fn from_source(spelling: impl Into<String>, parts: Vec<NamePart>) -> Self {
        Self {
            parts,
            source_spelling: Some(spelling.into().into_boxed_str()),
        }
    }

    /// Builds a single-segment name.
    pub fn single(part: impl Into<NamePart>) -> Self {
        Self {
            parts: vec![part.into()],
            source_spelling: None,
        }
    }

    /// Returns the segments in source order.
    pub fn parts(&self) -> &[NamePart] {
        &self.parts
    }

    /// Returns the Rust source spelling when it was preserved by the scanner.
    pub fn source_spelling(&self) -> Option<&str> {
        self.source_spelling.as_deref()
    }

    /// Returns the segments joined by `::`.
    pub fn as_path_string(&self) -> String {
        self.parts
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("::")
    }
}

impl Eq for CanonicalName {}

impl Hash for CanonicalName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.parts.hash(state);
    }
}

impl Ord for CanonicalName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.parts.cmp(&other.parts)
    }
}

impl PartialEq for CanonicalName {
    fn eq(&self, other: &Self) -> bool {
        self.parts == other.parts
    }
}

impl PartialOrd for CanonicalName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
