//! Package-prefix helper for the ergonomic C host surface.
//!
//! C has no namespaces, so every user-facing symbol (functions, constants,
//! friendly type aliases, associated wrappers) is prefixed with the library
//! (package) name so that multiple boltffi-generated headers can be included
//! in one translation unit without colliding. Prefixing is idempotent: a name
//! that already carries the package prefix is not doubled.

use boltffi_binding::Native;

use crate::core::RenderContext;

use super::super::name_style::Name;

/// The package prefix in the three C case forms.
pub struct PackagePrefix {
    member: String,
    constant: String,
    pascal: String,
}

impl PackagePrefix {
    /// Builds the prefix from the render context's binding package name.
    pub fn from_context(context: &RenderContext<Native>) -> Self {
        let name = context.bindings().package().name();
        Self {
            member: Name::new(name).member(),
            constant: Name::new(name).constant(),
            pascal: Name::new(name).r#type(),
        }
    }

    /// Prefixes a snake_case member/symbol name (idempotent).
    pub fn member(&self, raw: &str) -> String {
        let prefix = format!("{}_", self.member);
        if raw == self.member || raw.starts_with(&prefix) {
            raw.to_owned()
        } else {
            format!("{prefix}{raw}")
        }
    }

    /// Prefixes an UPPER_SNAKE macro/constant name (idempotent).
    pub fn constant(&self, raw: &str) -> String {
        let prefix = format!("{}_", self.constant);
        if raw == self.constant || raw.starts_with(&prefix) {
            raw.to_owned()
        } else {
            format!("{prefix}{raw}")
        }
    }

    /// Prefixes a PascalCase type name (idempotent), e.g. `Point` -> `DemoPoint`.
    pub fn type_name(&self, pascal: &str) -> String {
        if pascal.starts_with(&self.pascal) {
            pascal.to_owned()
        } else {
            format!("{}{}", self.pascal, pascal)
        }
    }
}
