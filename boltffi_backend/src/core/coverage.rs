use boltffi_binding::{CanonicalName, DeclarationRef, Surface};

/// Policy for unsupported declarations during backend rendering.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CoverageMode {
    /// Fails generation when any declaration cannot be rendered.
    #[default]
    Complete,
    /// Renders supported declarations and reports unsupported declarations.
    Partial,
}

/// Unsupported declarations found while rendering one backend target.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct CoverageReport {
    unsupported: Vec<UnsupportedDeclaration>,
}

impl CoverageReport {
    /// Creates an empty coverage report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns unsupported declarations in discovery order.
    pub fn unsupported(&self) -> &[UnsupportedDeclaration] {
        &self.unsupported
    }

    /// Returns whether every declaration was rendered.
    pub fn is_complete(&self) -> bool {
        self.unsupported.is_empty()
    }

    /// Adds one unsupported declaration.
    pub fn push(&mut self, unsupported: UnsupportedDeclaration) {
        self.unsupported.push(unsupported);
    }

    /// Appends another coverage report.
    pub fn append(&mut self, other: Self) {
        self.unsupported.extend(other.unsupported);
    }
}

/// One declaration a backend could not render.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct UnsupportedDeclaration {
    declaration: DeclarationLabel,
    reason: String,
}

impl UnsupportedDeclaration {
    /// Creates an unsupported declaration entry.
    pub fn new(declaration: DeclarationLabel, reason: impl Into<String>) -> Self {
        Self {
            declaration,
            reason: reason.into(),
        }
    }

    /// Returns the unsupported declaration.
    pub const fn declaration(&self) -> &DeclarationLabel {
        &self.declaration
    }

    /// Returns why the declaration was not rendered.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Display label for one binding declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DeclarationLabel {
    kind: &'static str,
    name: String,
}

impl DeclarationLabel {
    /// Creates a label from a binding declaration.
    pub fn from_ref<'decl, S: Surface>(declaration: DeclarationRef<'decl, S>) -> Self {
        let (kind, name) = match declaration {
            DeclarationRef::Record(record) => ("record", record.name()),
            DeclarationRef::Enum(enumeration) => ("enum", enumeration.name()),
            DeclarationRef::Function(function) => ("function", function.name()),
            DeclarationRef::Class(class) => ("class", class.name()),
            DeclarationRef::Callback(callback) => ("callback", callback.name()),
            DeclarationRef::Stream(stream) => ("stream", stream.name()),
            DeclarationRef::Constant(constant) => ("constant", constant.name()),
            DeclarationRef::CustomType(custom_type) => ("custom type", custom_type.name()),
        };
        Self::new(kind, name)
    }

    /// Creates a declaration label from a kind and canonical name.
    pub fn new(kind: &'static str, name: &CanonicalName) -> Self {
        Self {
            kind,
            name: name.as_path_string(),
        }
    }

    /// Returns the declaration kind.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Returns the canonical declaration name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Constructs a complete-coverage error identifying this declaration.
    pub(crate) fn coverage_error(&self, target: &'static str, reason: &str) -> crate::core::Error {
        crate::core::Error::IncompleteCoverage {
            target,
            reason: format!("{} {}: {reason}", self.kind, self.name),
        }
    }
}
