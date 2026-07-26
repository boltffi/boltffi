use askama::Template as AskamaTemplate;

use crate::{
    core::{BackendError, Result},
    target::jvm::{
        DesktopLoader, NativeLibraries,
        resource::{PLATFORMS, Platform},
    },
};

use super::{
    super::{
        name_style::JavaPackage,
        syntax::{Identifier, StringLiteral},
    },
    Loader,
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/loader.java", escape = "none")]
struct LoaderTemplate<'libraries> {
    owner: &'libraries Identifier,
    runtime_owner: &'libraries Identifier,
    libraries: &'libraries LibraryLiterals,
}

#[derive(AskamaTemplate)]
#[template(path = "target/java/runtime/desktop.java", escape = "none")]
struct DesktopTemplate<'runtime> {
    package: &'runtime JavaPackage,
    runtime_owner: &'runtime Identifier,
    resource_platforms: &'static [Platform],
}

struct LibraryLiterals {
    android: StringLiteral,
    desktop_jni: StringLiteral,
    desktop_fallback: StringLiteral,
    desktop_loader: DesktopLoader,
}

impl LibraryLiterals {
    fn new(libraries: &NativeLibraries) -> Self {
        Self {
            android: StringLiteral::new(libraries.android().as_str()),
            desktop_jni: StringLiteral::new(libraries.desktop_jni().as_str()),
            desktop_fallback: StringLiteral::new(libraries.desktop_fallback().as_str()),
            desktop_loader: libraries.desktop_loader(),
        }
    }

    fn android_literal(&self) -> &StringLiteral {
        &self.android
    }

    fn desktop_jni_literal(&self) -> &StringLiteral {
        &self.desktop_jni
    }

    fn desktop_fallback_literal(&self) -> &StringLiteral {
        &self.desktop_fallback
    }

    fn bundled_desktop_loader(&self) -> bool {
        self.desktop_loader.loads_bundled()
    }

    fn system_desktop_loader(&self) -> bool {
        self.desktop_loader.loads_system()
    }
}

impl Loader<'_> {
    pub fn new(
        owner: Identifier,
        runtime_owner: Identifier,
        libraries: &NativeLibraries,
    ) -> Loader<'_> {
        Loader {
            owner,
            runtime_owner,
            libraries,
        }
    }

    pub fn render(&self) -> Result<String> {
        let libraries = LibraryLiterals::new(self.libraries);
        LoaderTemplate {
            owner: &self.owner,
            runtime_owner: &self.runtime_owner,
            libraries: &libraries,
        }
        .render()
        .map_err(BackendError::from)
    }

    pub fn desktop_source(&self, package: &JavaPackage) -> Result<Option<String>> {
        self.libraries
            .desktop_loader()
            .loads_bundled()
            .then(|| {
                DesktopTemplate {
                    package,
                    runtime_owner: &self.runtime_owner,
                    resource_platforms: PLATFORMS,
                }
                .render()
                .map_err(BackendError::from)
            })
            .transpose()
    }
}
