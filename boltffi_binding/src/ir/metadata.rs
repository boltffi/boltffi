use serde::{Deserialize, Serialize};

use crate::CanonicalName;

/// Documentation text preserved from the Rust source.
///
/// Stored verbatim, including newlines and Markdown the author wrote. Each
/// target language reformats the text into its own documentation syntax
/// (KDoc for Kotlin, Javadoc for Java, docstrings for Python, and so on).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocComment(String);

impl DocComment {
    /// Stores documentation text.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Returns the text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Deprecation metadata preserved from the Rust source.
///
/// `since` is the version of the Rust crate that introduced the
/// deprecation; `message` is the human-readable reason. Either may be
/// absent if the source `#[deprecated]` attribute did not provide it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DeprecationInfo {
    message: Option<String>,
    since: Option<String>,
}

impl DeprecationInfo {
    /// Builds deprecation metadata.
    pub fn new(message: Option<String>, since: Option<String>) -> Self {
        Self { message, since }
    }

    /// Returns the deprecation message.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Returns the version that introduced the deprecation.
    pub fn since(&self) -> Option<&str> {
        self.since.as_deref()
    }
}

/// A resolved integer literal value.
///
/// The source expression has already been evaluated to a concrete number,
/// so generated bindings can render the value without parsing Rust syntax.
/// Stored as `i128` so any signed or unsigned integer up to 64 bits
/// round-trips without loss.
///
/// Serialized as the `i64` or `u64` it fits, never as `i128`: serde buffers
/// the content of a flattened or tagged container (a class's callables are
/// flattened into it), and that buffer has no `i128`, so a constructor or
/// method default would not decode.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IntegerValue(i128);

impl Serialize for IntegerValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Ok(value) = i64::try_from(self.0) {
            serializer.serialize_i64(value)
        } else if let Ok(value) = u64::try_from(self.0) {
            serializer.serialize_u64(value)
        } else {
            Err(serde::ser::Error::custom(format!(
                "integer default {} does not fit in 64 bits",
                self.0
            )))
        }
    }
}

impl<'de> Deserialize<'de> for IntegerValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = IntegerValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an integer of at most 64 bits")
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(IntegerValue(value.into()))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(IntegerValue(value.into()))
            }

            fn visit_i128<E: serde::de::Error>(self, value: i128) -> Result<Self::Value, E> {
                Ok(IntegerValue(value))
            }

            fn visit_u128<E: serde::de::Error>(self, value: u128) -> Result<Self::Value, E> {
                i128::try_from(value)
                    .map(IntegerValue)
                    .map_err(|_| E::custom("integer default does not fit in 128 bits"))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl IntegerValue {
    /// Stores a resolved integer value.
    pub const fn new(value: i128) -> Self {
        Self(value)
    }

    /// Returns the value.
    pub const fn get(self) -> i128 {
        self.0
    }
}

/// A resolved floating-point literal value, stored as a bit pattern.
///
/// Storing the IEEE-754 bits rather than the `f64` directly makes equality,
/// hashing, and serialization deterministic: `+0.0` and `-0.0` are not
/// equal, NaNs compare equal to themselves, and round-tripping through
/// serde does not lose the distinction between two NaN payloads.
///
/// # Example
///
/// `FloatValue::from_f64(1.5).bits() == 0x3FF8000000000000`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FloatValue(u64);

impl FloatValue {
    /// Stores an `f64` literal by its IEEE-754 bit pattern.
    pub const fn from_f64(value: f64) -> Self {
        Self(value.to_bits())
    }

    /// Returns the value as `f64`.
    pub const fn to_f64(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Returns the underlying bit pattern.
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// A literal value generated bindings can emit without calling Rust.
///
/// The classifier admits a default only when the value can be expressed
/// directly in every reasonable target language: primitive scalars,
/// strings, named enum variants, and the absence of a value. Anything that
/// would require running Rust at binding time (a `Vec::new()` call, a
/// computed expression) is not stored here.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DefaultValue {
    /// Boolean literal.
    Bool(bool),
    /// Integer literal.
    Integer(IntegerValue),
    /// Floating-point literal.
    Float(FloatValue),
    /// UTF-8 string literal.
    String(String),
    /// Named enum variant.
    EnumVariant {
        /// Enum carrying the variant.
        enum_name: CanonicalName,
        /// Variant selected as the default.
        variant_name: CanonicalName,
    },
    /// Absence of a value, for optional types.
    Null,
}

/// Source-derived metadata attached to a top-level declaration.
///
/// Documentation and deprecation are the two facts the binding contract
/// preserves at declaration scope. Everything else (source spans,
/// attributes, attribute arguments) is consumed by the classifier or
/// dropped.
///
/// # Example
///
/// A Rust `#[deprecated(since = "0.24.0", note = "use open instead")]`
/// attribute on an exported function becomes deprecation metadata here.
/// The backend can render that as a target-language deprecation annotation
/// without parsing Rust attributes.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DeclMeta {
    doc: Option<DocComment>,
    deprecated: Option<DeprecationInfo>,
}

impl DeclMeta {
    /// Builds declaration metadata.
    pub fn new(doc: Option<DocComment>, deprecated: Option<DeprecationInfo>) -> Self {
        Self { doc, deprecated }
    }

    /// Returns the doc comment.
    pub fn doc(&self) -> Option<&DocComment> {
        self.doc.as_ref()
    }

    /// Returns the deprecation info.
    pub fn deprecated(&self) -> Option<&DeprecationInfo> {
        self.deprecated.as_ref()
    }
}

/// Source-derived metadata attached to a field, parameter, or variant.
///
/// Same shape as [`DeclMeta`] plus a default value, which fields,
/// parameters, and variants can carry and top-level declarations cannot.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ElementMeta {
    doc: Option<DocComment>,
    deprecated: Option<DeprecationInfo>,
    default: Option<DefaultValue>,
}

impl ElementMeta {
    /// Builds element metadata.
    pub fn new(
        doc: Option<DocComment>,
        deprecated: Option<DeprecationInfo>,
        default: Option<DefaultValue>,
    ) -> Self {
        Self {
            doc,
            deprecated,
            default,
        }
    }

    /// Returns the doc comment.
    pub fn doc(&self) -> Option<&DocComment> {
        self.doc.as_ref()
    }

    /// Returns the deprecation info.
    pub fn deprecated(&self) -> Option<&DeprecationInfo> {
        self.deprecated.as_ref()
    }

    /// Returns the default value.
    pub fn default(&self) -> Option<&DefaultValue> {
        self.default.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::{DefaultValue, IntegerValue};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Inner {
        default: DefaultValue,
    }

    /// A class flattens its callables, which makes serde buffer them.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Flattened {
        #[serde(flatten)]
        inner: Inner,
    }

    #[test]
    fn integer_defaults_round_trip_through_a_flattened_container() {
        for value in [0, -1, i128::from(i64::MIN), i128::from(u64::MAX)] {
            let flattened = Flattened {
                inner: Inner {
                    default: DefaultValue::Integer(IntegerValue::new(value)),
                },
            };
            let json = serde_json::to_string(&flattened).expect("serialize");
            assert_eq!(json, format!(r#"{{"default":{{"Integer":{value}}}}}"#));
            let decoded: Flattened = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, flattened);
        }
    }

    #[test]
    fn an_integer_default_beyond_64_bits_does_not_serialize() {
        let value = DefaultValue::Integer(IntegerValue::new(i128::from(u64::MAX) + 1));
        assert!(serde_json::to_string(&value).is_err());
    }
}
