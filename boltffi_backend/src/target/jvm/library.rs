use std::fmt;

use crate::core::{Error, Result};

/// A logical native library name passed to JVM runtime loaders.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LibraryName(String);

/// Desktop native-library loading policy for a JVM host.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DesktopLoader {
    /// Loads bundled native resources before the system fallback.
    #[default]
    Bundled,
    /// Loads the desktop fallback through `System.loadLibrary`.
    System,
    /// Omits desktop native-library loading.
    None,
}

/// Native libraries loaded by a JVM host.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeLibraries {
    android: LibraryName,
    desktop_jni: LibraryName,
    desktop_fallback: LibraryName,
    desktop_loader: DesktopLoader,
}

impl LibraryName {
    /// Parses a JVM native-library load name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        match !name.is_empty()
            && !name
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\'))
        {
            true => Ok(Self(name)),
            false => Err(Error::UnsupportedTarget {
                target: "jvm",
                shape: "native library name",
            }),
        }
    }

    /// Returns the runtime load name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LibraryName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl DesktopLoader {
    /// Returns whether bundled desktop resources are loaded.
    pub const fn loads_bundled(self) -> bool {
        matches!(self, Self::Bundled)
    }

    /// Returns whether only the system desktop fallback is loaded.
    pub const fn loads_system(self) -> bool {
        matches!(self, Self::System)
    }
}

impl NativeLibraries {
    /// Derives native library names from a Cargo artifact name.
    pub fn from_artifact(artifact: impl Into<String>) -> Result<Self> {
        let artifact = LibraryName::parse(artifact)?;
        let desktop_jni = LibraryName::parse(format!("{}_jni", artifact.as_str()))?;
        Ok(Self {
            android: artifact.clone(),
            desktop_jni,
            desktop_fallback: artifact,
            desktop_loader: DesktopLoader::default(),
        })
    }

    /// Creates the default BoltFFI native library names.
    pub fn boltffi() -> Result<Self> {
        Self::from_artifact("boltffi")
    }

    /// Selects the Android native library.
    pub fn with_android(mut self, library: LibraryName) -> Self {
        self.android = library;
        self
    }

    /// Selects the desktop JNI wrapper library.
    pub fn with_desktop_jni(mut self, library: LibraryName) -> Self {
        self.desktop_jni = library;
        self
    }

    /// Selects the desktop fallback library.
    pub fn with_desktop_fallback(mut self, library: LibraryName) -> Self {
        self.desktop_fallback = library;
        self
    }

    /// Selects the desktop loading policy.
    pub fn with_desktop_loader(mut self, loader: DesktopLoader) -> Self {
        self.desktop_loader = loader;
        self
    }

    /// Returns the Android native library.
    pub fn android(&self) -> &LibraryName {
        &self.android
    }

    /// Returns the desktop JNI wrapper library.
    pub fn desktop_jni(&self) -> &LibraryName {
        &self.desktop_jni
    }

    /// Returns the desktop fallback library.
    pub fn desktop_fallback(&self) -> &LibraryName {
        &self.desktop_fallback
    }

    /// Returns the desktop loading policy.
    pub const fn desktop_loader(&self) -> DesktopLoader {
        self.desktop_loader
    }
}

#[cfg(test)]
mod tests {
    use super::{DesktopLoader, LibraryName, NativeLibraries};

    #[test]
    fn accepts_logical_loader_names() {
        assert_eq!(
            LibraryName::parse("native-$core").unwrap().as_str(),
            "native-$core"
        );
    }

    #[test]
    fn derives_loader_names_from_one_cargo_artifact() {
        let libraries = NativeLibraries::from_artifact("demo_core").unwrap();

        assert_eq!(libraries.android().as_str(), "demo_core");
        assert_eq!(libraries.desktop_jni().as_str(), "demo_core_jni");
        assert_eq!(libraries.desktop_fallback().as_str(), "demo_core");
    }

    #[test]
    fn rejects_empty_control_and_path_separated_loader_names() {
        ["", "native/core", "native\\core", "native\ncore"]
            .into_iter()
            .for_each(|name| assert!(LibraryName::parse(name).is_err()));
    }

    #[test]
    fn preserves_existing_kotlin_loader_name_contract() {
        ["native.core", "nätive", "native:core", "native-$core"]
            .into_iter()
            .for_each(|name| assert_eq!(LibraryName::parse(name).unwrap().as_str(), name));
    }

    #[test]
    fn owns_desktop_loading_policy() {
        let libraries = NativeLibraries::boltffi()
            .unwrap()
            .with_desktop_jni(LibraryName::parse("NativeCore").unwrap())
            .with_desktop_fallback(LibraryName::parse("nativecore").unwrap())
            .with_desktop_loader(DesktopLoader::System);

        assert!(libraries.desktop_loader().loads_system());
        assert!(!libraries.desktop_loader().loads_bundled());
    }
}
