use serde::{Deserialize, Serialize};

use crate::{
    ClassId, DeprecationInfo, DocComment, MethodDef, Source, SourceName, SourceSpan, UserAttr,
};

/// A class-style Rust object exported through BoltFFI.
///
/// A class groups associated functions and methods around an owned
/// Rust value. Associated functions that return `Self` stay as methods here;
/// binding layers can present them as creation entry points later.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    /// Stable class identity derived from the canonical Rust path.
    pub id: ClassId,
    /// Source class name.
    pub name: SourceName,
    /// Methods attached to the class.
    pub methods: Vec<MethodDef>,
    /// Thread-safety policy collected from exported class impl blocks.
    #[serde(default)]
    pub thread_safety: ClassThreadSafety,
    /// User attributes preserved from the class declaration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    #[serde(default, skip_serializing_if = "Source::is_exported")]
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

/// Thread-safety policy for an exported class.
///
/// Classes require `Send + Sync` unless every exported impl block declares
/// single-threaded access.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ClassThreadSafety {
    /// The Rust class type must implement `Send + Sync`.
    #[default]
    RequireSendSync,
    /// The class is exported without a `Send + Sync` assertion.
    UnsafeSingleThreaded,
}

impl ClassThreadSafety {
    /// Merges policies collected from multiple impl blocks.
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::UnsafeSingleThreaded, Self::UnsafeSingleThreaded) => Self::UnsafeSingleThreaded,
            _ => Self::RequireSendSync,
        }
    }
}

impl ClassDef {
    /// Builds an empty class definition.
    ///
    /// The `id` parameter is the stable class ID. The `name` parameter is the
    /// canonical source name.
    ///
    /// Returns a class with no methods, attributes, or docs.
    pub fn new(id: ClassId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            methods: Vec::new(),
            thread_safety: ClassThreadSafety::default(),
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, to_value};

    use super::*;
    use crate::CanonicalName;

    #[test]
    fn missing_thread_safety_deserializes_to_required_send_sync() {
        let mut value = to_value(ClassDef::new(
            ClassId::new("demo::Engine"),
            CanonicalName::single("Engine"),
        ))
        .expect("class serializes");
        let Value::Object(fields) = &mut value else {
            panic!("serialized class must be an object");
        };
        fields.remove("thread_safety");

        let class = serde_json::from_value::<ClassDef>(value).expect("class deserializes");

        assert_eq!(class.thread_safety, ClassThreadSafety::RequireSendSync);
    }
}
