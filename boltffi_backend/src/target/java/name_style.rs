use std::{fmt, path::PathBuf};

use boltffi_binding::{CanonicalName, NamePart};

use crate::{
    core::{Error, Result, name_case},
    target::java::{
        JavaVersion,
        syntax::{Identifier, TypeIdentifier, TypeName},
    },
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PathComponent<T>(T);

trait PathValue {
    fn as_path_component(&self) -> &str;
}

/// A Java package name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JavaPackage {
    segments: Vec<PathComponent<Identifier>>,
}

/// A Java source file stem.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JavaFile {
    name: PathComponent<TypeIdentifier>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name {
    source: CanonicalName,
}

impl JavaPackage {
    /// Parses a dotted package name using Java 8 lexical rules.
    pub fn parse(package: impl Into<String>) -> Result<Self> {
        Self::parse_for(package, JavaVersion::JAVA_8)
    }

    /// Parses a dotted Java package name for a Java release.
    pub fn parse_for(package: impl Into<String>, version: JavaVersion) -> Result<Self> {
        let package = package.into();
        let segments = package
            .split('.')
            .map(|segment| Identifier::parse_for(segment, version))
            .map(|identifier| {
                identifier.and_then(|identifier| {
                    PathComponent::new(identifier).ok_or(Error::UnsupportedTarget {
                        target: "java",
                        shape: "generated source path component",
                    })
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if segments
            .first()
            .is_some_and(|segment| segment.as_str() == "java")
        {
            return Err(Error::InvalidJvmPackageName { name: package });
        }
        Ok(Self { segments })
    }

    /// Returns the package directory path.
    pub fn directory(&self) -> PathBuf {
        self.segments
            .iter()
            .map(PathComponent::as_str)
            .collect::<PathBuf>()
    }

    /// Validates every package segment for a Java release.
    pub fn validate(&self, version: JavaVersion) -> Result<()> {
        self.segments
            .iter()
            .try_for_each(|segment| segment.value().validate(version))
    }

    /// Qualifies a generated Java type inside this package.
    pub fn type_name(&self, name: TypeIdentifier) -> TypeName {
        TypeName::qualified(
            self.segments
                .iter()
                .map(PathComponent::value)
                .cloned()
                .collect(),
            name,
        )
    }
}

impl fmt::Display for JavaPackage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            &self
                .segments
                .iter()
                .map(PathComponent::as_str)
                .collect::<Vec<_>>()
                .join("."),
        )
    }
}

impl JavaFile {
    /// Parses a source file stem using Java 8 lexical rules.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        Self::parse_for(name, JavaVersion::default())
    }

    /// Parses a Java source file stem for a Java release.
    pub fn parse_for(name: impl Into<String>, version: JavaVersion) -> Result<Self> {
        TypeIdentifier::parse(name, version).and_then(|name| {
            PathComponent::new(name)
                .map(|name| Self { name })
                .ok_or(Error::UnsupportedTarget {
                    target: "java",
                    shape: "generated source path component",
                })
        })
    }

    /// Returns the generated source path inside a package.
    pub fn path(&self, package: &JavaPackage) -> PathBuf {
        package
            .directory()
            .join(format!("{}.java", self.name.as_str()))
    }

    /// Returns the source file stem.
    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }

    /// Validates the public class name for a Java release.
    pub fn validate(&self, version: JavaVersion) -> Result<()> {
        self.name.value().validate(version)
    }
}

impl fmt::Display for JavaFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name.as_str())
    }
}

impl Name {
    pub fn new(source: &CanonicalName) -> Self {
        Self {
            source: source.clone(),
        }
    }

    pub fn function(&self, version: JavaVersion) -> Result<Identifier> {
        Identifier::escape_for(self.lower_camel(), version)
    }

    pub fn parameter(&self, version: JavaVersion) -> Result<Identifier> {
        Identifier::escape_for(self.lower_camel(), version)
    }

    pub fn type_name(&self, version: JavaVersion) -> Result<TypeIdentifier> {
        TypeIdentifier::escape(name_case::upper_camel(&self.source), version)
    }

    pub fn variant(&self, version: JavaVersion) -> Result<TypeIdentifier> {
        self.type_name(version)
    }

    pub fn enum_entry(&self, version: JavaVersion) -> Result<Identifier> {
        Identifier::escape_for(
            self.source
                .parts()
                .iter()
                .map(NamePart::as_str)
                .map(str::to_ascii_uppercase)
                .collect::<Vec<_>>()
                .join("_"),
            version,
        )
    }

    pub fn generated(&self, suffix: &str, version: JavaVersion) -> Result<Identifier> {
        Identifier::parse_for(
            format!("__boltffi_{}_{suffix}", self.lower_camel()),
            version,
        )
    }

    fn lower_camel(&self) -> String {
        name_case::lower_camel(&self.source)
    }
}

impl<T> PathComponent<T>
where
    T: PathValue,
{
    fn new(value: T) -> Option<Self> {
        Self::portable(value.as_path_component()).then_some(Self(value))
    }

    fn value(&self) -> &T {
        &self.0
    }

    fn as_str(&self) -> &str {
        self.0.as_path_component()
    }

    fn portable(component: &str) -> bool {
        !component.is_empty()
            && !matches!(component, "." | "..")
            && !component.ends_with([' ', '.'])
            && !component
                .chars()
                .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
            && !Self::windows_device(component)
    }

    fn windows_device(component: &str) -> bool {
        let stem = component
            .split_once('.')
            .map_or(component, |(stem, _)| stem)
            .to_uppercase();
        matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|port| {
                matches!(
                    port,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
    }
}

impl PathValue for Identifier {
    fn as_path_component(&self) -> &str {
        self.as_str()
    }
}

impl PathValue for TypeIdentifier {
    fn as_path_component(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use boltffi_binding::{CanonicalName, NamePart};

    use super::{JavaFile, JavaPackage, Name};
    use crate::target::java::JavaVersion;

    #[test]
    fn maps_validated_names_to_java_roles() {
        let source = CanonicalName::new(vec![NamePart::new("http"), NamePart::new("status")]);
        let name = Name::new(&source);
        assert_eq!(
            name.function(JavaVersion::JAVA_8).unwrap().to_string(),
            "httpStatus"
        );
        assert_eq!(
            name.parameter(JavaVersion::JAVA_8).unwrap().to_string(),
            "httpStatus"
        );
    }

    #[test]
    fn escapes_java_keywords_after_casing() {
        let name = Name::new(&CanonicalName::single("class"));
        assert_eq!(
            name.function(JavaVersion::JAVA_8).unwrap().to_string(),
            "_class"
        );
    }

    #[test]
    fn derives_java_names_from_canonical_parts() {
        [
            (vec!["http", "request"], "httpRequest"),
            (vec!["cafe", "über"], "cafeÜber"),
            (vec!["leading", "name"], "leadingName"),
            (vec!["type"], "type"),
            (vec!["class"], "_class"),
        ]
        .into_iter()
        .try_for_each(|(parts, expected)| {
            let name = Name::new(&CanonicalName::new(
                parts.into_iter().map(NamePart::new).collect(),
            ));
            assert_eq!(
                name.function(JavaVersion::JAVA_17).unwrap().as_str(),
                expected
            );
            Ok::<_, crate::Error>(())
        })
        .unwrap();
    }

    #[test]
    fn validates_package_segments_and_builds_source_paths() {
        let package = JavaPackage::parse("com.module.bindings").unwrap();
        let file = JavaFile::parse("Native").unwrap();
        assert_eq!(package.to_string(), "com.module.bindings");
        assert_eq!(
            file.path(&package),
            PathBuf::from("com/module/bindings/Native.java")
        );
        assert!(JavaPackage::parse("com.class.bindings").is_err());
        assert!(
            JavaPackage::parse("com._.bindings")
                .unwrap()
                .validate(JavaVersion::JAVA_9)
                .is_err()
        );
    }

    #[test]
    fn validates_file_type_names_for_the_selected_release() {
        let file = JavaFile::parse("record").unwrap();
        assert!(file.validate(JavaVersion::new(15).unwrap()).is_ok());
        assert!(file.validate(JavaVersion::JAVA_16).is_err());
    }

    #[test]
    fn rejects_nonportable_generated_paths() {
        ["NUL", "con", "COM1", "LPT³", "CONIN$", "CONOUT$"]
            .into_iter()
            .for_each(|name| assert!(JavaFile::parse(name).is_err()));
        ["com.aux.bindings", "com.prn.bindings", "com.lpt1.bindings"]
            .into_iter()
            .for_each(|name| assert!(JavaPackage::parse(name).is_err()));
    }

    #[test]
    fn rejects_the_java_runtime_namespace_only() {
        assert!(JavaPackage::parse("java.boltffi.demo").is_err());
        assert!(JavaPackage::parse("javax.boltffi.demo").is_ok());
        assert!(JavaPackage::parse("Java.boltffi.demo").is_ok());
    }
}
