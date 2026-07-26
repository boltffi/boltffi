//! JVM class paths used by generated JNI glue.
//!
//! The generated bridge talks about the same class in several languages. Java
//! binary names use dotted package names, JNI lookup uses slash-separated paths,
//! and exported native methods use the class inside an escaped `Java_*` symbol.
//!
//! This module stores one validated class path and exposes those spellings from
//! that single value. Callers do not split package strings or hand-roll JNI
//! lookup paths.

use std::fmt;

use boltffi_binding::{CanonicalName, ClosureSignature};

use crate::{bridge::jni::name::JvmNameSegment, core::Result};

/// Fully qualified JVM class that owns generated native methods.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JvmClassPath {
    package: Vec<JvmNameSegment>,
    class: JvmNameSegment,
}

impl JvmClassPath {
    /// Creates a JVM class path from a package name and class name.
    pub fn new(package: impl Into<String>, class: impl Into<String>) -> Result<Self> {
        let package = package.into();
        let package = match package.is_empty() {
            true => Vec::new(),
            false => package
                .split('.')
                .map(JvmNameSegment::package)
                .collect::<Result<Vec<_>>>()?,
        };
        Self::from_parts(package, JvmNameSegment::class(class)?)
    }

    /// Returns the dotted JVM binary name.
    pub fn as_java_path(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(JvmNameSegment::as_str)
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Returns the unqualified JVM class name.
    pub fn class_name(&self) -> &str {
        self.class.as_str()
    }

    /// Returns the slash-separated class name used by JNI class lookup.
    pub fn as_jni_class_name(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(JvmNameSegment::as_str)
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Creates the generated callback bridge class in the same JVM package.
    pub fn callback_class(&self, callback: &CanonicalName) -> Result<Self> {
        Self::from_parts(
            self.package.clone(),
            JvmNameSegment::callback_class(callback)?,
        )
    }

    /// Creates the generated closure bridge class in the same JVM package.
    pub fn closure_class(&self, signature: &ClosureSignature) -> Result<Self> {
        Self::from_parts(
            self.package.clone(),
            JvmNameSegment::closure_class(signature)?,
        )
    }

    /// Creates a class in the same JVM package.
    pub fn sibling_class(&self, class: impl Into<String>) -> Result<Self> {
        Self::from_parts(self.package.clone(), JvmNameSegment::class(class)?)
    }

    /// Returns the class path as the prefix used by a JNI exported symbol.
    pub fn jni_prefix(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(JvmNameSegment::jni_escape)
            .collect::<Vec<_>>()
            .join("_")
    }

    fn from_parts(package: Vec<JvmNameSegment>, class: JvmNameSegment) -> Result<Self> {
        let class_path = Self { package, class };
        class_path.validate_jni_prefix()?;
        Ok(class_path)
    }

    fn validate_jni_prefix(&self) -> Result<()> {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .try_for_each(JvmNameSegment::validate_jni_escape)
    }
}

impl fmt::Display for JvmClassPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_java_path())
    }
}
