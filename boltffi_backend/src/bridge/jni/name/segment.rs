//! Validated JVM binary-name segments.
//!
//! Package segments, class names, generated callback classes, and generated
//! closure classes share the JVM's unqualified binary-name constraints. They
//! may contain characters rejected by Java source identifiers, but cannot
//! contain `.`, `;`, `[`, or `/`.
//!
//! This module owns that validation and the generated callback and closure
//! names.

use boltffi_binding::{CanonicalName, ClosureSignature};

use crate::{
    bridge::jni::name::JniSymbolName,
    core::{Error, Result, name_case},
};

/// One JVM package or class-name segment.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JvmNameSegment {
    name: String,
}

impl JvmNameSegment {
    /// Returns the JVM source spelling.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub(in crate::bridge::jni::name) fn package(name: &str) -> Result<Self> {
        Self::parse(name.to_owned(), |name| Error::InvalidJvmPackageName {
            name,
        })
    }

    pub(in crate::bridge::jni::name) fn class(name: impl Into<String>) -> Result<Self> {
        Self::parse(name.into(), |name| Error::InvalidJvmClassName { name })
    }

    pub(in crate::bridge::jni::name) fn callback_class(callback: &CanonicalName) -> Result<Self> {
        Self::class(format!("{}Callbacks", Self::canonical_class(callback)))
    }

    pub(in crate::bridge::jni::name) fn closure_class(
        signature: &ClosureSignature,
    ) -> Result<Self> {
        Self::class(format!("Closure{}Callbacks", signature.as_str()))
    }

    pub(in crate::bridge::jni::name) fn jni_escape(&self) -> String {
        JniSymbolName::escape_validated_part(&self.name)
    }

    pub(in crate::bridge::jni::name) fn validate_jni_escape(&self) -> Result<()> {
        JniSymbolName::escape_part(&self.name).map(drop)
    }

    fn canonical_class(name: &CanonicalName) -> String {
        name_case::upper_camel(name)
    }

    fn parse(name: String, error: impl FnOnce(String) -> Error) -> Result<Self> {
        if Self::valid(&name) {
            Ok(Self { name })
        } else {
            Err(error(name))
        }
    }

    fn valid(name: &str) -> bool {
        !name.is_empty()
            && !name
                .chars()
                .any(|character| matches!(character, '.' | ';' | '[' | '/'))
    }
}

#[cfg(test)]
mod tests {
    use super::JvmNameSegment;

    #[test]
    fn accepts_jvm_binary_name_segments() {
        assert!(JvmNameSegment::package("δοκιμή").is_ok());
        assert!(JvmNameSegment::package("東京").is_ok());
        assert!(JvmNameSegment::class("Модуль$Native").is_ok());
        assert!(JvmNameSegment::class("9-€-😀").is_ok());
    }

    #[test]
    fn rejects_jvm_binary_name_separators() {
        assert!(JvmNameSegment::package("").is_err());
        assert!(JvmNameSegment::package("nested.package").is_err());
        assert!(JvmNameSegment::class("Native/Class").is_err());
        assert!(JvmNameSegment::class("Native;Class").is_err());
        assert!(JvmNameSegment::class("Native[Class").is_err());
    }
}
