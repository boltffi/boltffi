use thiserror::Error as ThisError;

use crate::core::{BindingCapability, BridgeCapability, CapabilityStatus};

/// Result type used by backend rendering.
pub type Result<T> = std::result::Result<T, BackendError>;

/// Short name for backend failures.
pub type Error = BackendError;

/// Failure while composing or rendering a backend target.
#[derive(Clone, Debug, Eq, ThisError, PartialEq)]
#[non_exhaustive]
pub enum BackendError {
    /// A host cannot render a binding declaration shape present in the contract.
    #[error("backend `{target}` does not support binding capability {capability:?}: {status:?}")]
    BindingCapability {
        /// Backend target name.
        target: &'static str,
        /// Required binding capability.
        capability: BindingCapability,
        /// Support status advertised by the host.
        status: CapabilityStatus,
    },
    /// A host requires a bridge capability the selected bridge stack does not provide.
    #[error("backend `{target}` requires bridge capability {capability:?}: {status:?}")]
    BridgeCapability {
        /// Backend target name.
        target: &'static str,
        /// Required bridge capability.
        capability: BridgeCapability,
        /// Support status advertised by the bridge contract.
        status: CapabilityStatus,
    },
    /// Complete rendering was requested but some declarations were skipped.
    #[error("backend `{target}` did not render every declaration: {reason}")]
    IncompleteCoverage {
        /// Backend target name.
        target: &'static str,
        /// First unsupported declaration summary.
        reason: String,
    },
    /// A generated file path was empty.
    #[error("generated file path cannot be empty")]
    EmptyFilePath,
    /// Anonymous output was assembled with a layout that does not name exactly one file.
    #[error("anonymous emitted output requires a single-file layout")]
    AnonymousOutputNeedsSingleFile,
    /// A rendered declaration did not match any generated file plan.
    #[error("no generated file plan matched {declaration}")]
    UnmatchedFilePlan {
        /// Declaration kind that had no matching file plan.
        declaration: &'static str,
    },
    /// The C ABI bridge cannot render the supplied binding shape.
    #[error("C ABI bridge cannot render {shape}")]
    UnsupportedCAbi {
        /// Binding shape that has no C ABI rendering.
        shape: &'static str,
    },
    /// A bridge layer cannot render the supplied lower bridge shape.
    #[error("{bridge} bridge cannot render {shape}")]
    UnsupportedBridge {
        /// Bridge layer name.
        bridge: &'static str,
        /// Shape that has no bridge rendering.
        shape: &'static str,
    },
    /// A backend saw a binding shape that should not reach that rendering path.
    #[error("{layer} received unexpected binding shape `{shape}`")]
    UnexpectedBindingShape {
        /// Backend layer that received the unexpected shape.
        layer: &'static str,
        /// Unexpected binding shape.
        shape: &'static str,
    },
    /// A configured custom type mapping did not match a custom type declaration.
    #[error("{target} target has no custom type named `{custom_type}`")]
    UnknownCustomTypeMapping {
        /// Host target name.
        target: &'static str,
        /// Configured mapping key.
        custom_type: String,
    },
    /// A bridge contract index was missing data that construction should have recorded.
    #[error("{bridge} bridge contract invariant failed: {invariant}")]
    BrokenBridgeContract {
        /// Bridge contract name.
        bridge: &'static str,
        /// Missing or inconsistent contract invariant.
        invariant: &'static str,
    },
    /// A host target cannot render the supplied binding shape.
    #[error("{target} target cannot render {shape}")]
    UnsupportedTarget {
        /// Host target name.
        target: &'static str,
        /// Binding shape that has no target rendering.
        shape: &'static str,
    },
    /// A generated C identifier was invalid.
    #[error("invalid C identifier `{identifier}`")]
    InvalidCIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    /// A generated C# identifier was invalid.
    #[error("invalid C# identifier `{identifier}`")]
    InvalidCSharpIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    /// A generated C# namespace was invalid.
    #[error("invalid C# namespace `{namespace}`")]
    InvalidCSharpNamespace {
        /// Invalid namespace text.
        namespace: String,
    },
    /// A generated C include path was invalid.
    #[error("invalid C include path `{path}`")]
    InvalidCIncludePath {
        /// Invalid include path text.
        path: String,
    },
    /// A generated JVM package name was invalid.
    #[error("invalid JVM package name `{name}`")]
    InvalidJvmPackageName {
        /// Invalid package name text.
        name: String,
    },
    /// A generated JVM class name was invalid.
    #[error("invalid JVM class name `{name}`")]
    InvalidJvmClassName {
        /// Invalid class name text.
        name: String,
    },
    /// A generated CPython method name was invalid.
    #[error("invalid CPython method name `{name}`")]
    InvalidPythonMethodName {
        /// Invalid method name text.
        name: String,
    },
    /// A generated Python identifier was invalid.
    #[error("invalid Python identifier `{identifier}`")]
    InvalidPythonIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    /// A generated Python package module name was invalid.
    #[error("invalid Python package module name `{name}`")]
    InvalidPythonPackageModule {
        /// Invalid module name text.
        name: String,
    },
    /// A generated Kotlin identifier was invalid.
    #[error("invalid Kotlin identifier `{identifier}`")]
    InvalidKotlinIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    /// A generated Java identifier was invalid.
    #[error("invalid Java identifier `{identifier}`")]
    InvalidJavaIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    /// A generated Swift identifier was invalid.
    #[error("invalid Swift identifier `{identifier}`")]
    InvalidSwiftIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    #[allow(missing_docs)]
    #[error("invalid TypeScript identifier `{identifier}`")]
    InvalidTypeScriptIdentifier { identifier: String },
    /// Two generated Python declarations require the same name in one scope.
    #[error("python name collision in {scope}: `{name}` is used by {existing} and {colliding}")]
    PythonNameCollision {
        /// Python scope where the collision was found.
        scope: String,
        /// Generated Python name used more than once.
        name: String,
        /// Declaration that claimed the generated name first.
        existing: String,
        /// Declaration that collided with the existing name.
        colliding: String,
    },
    /// Two generated Kotlin declarations require the same name in one scope.
    #[error("kotlin name collision in {scope}: `{name}` is already used")]
    KotlinNameCollision {
        /// Kotlin scope where the collision was found.
        scope: String,
        /// Generated Kotlin name used more than once.
        name: String,
    },
    /// Two generated Java declarations require the same name in one scope.
    #[error("java name collision in {scope}: `{name}` is already used")]
    JavaNameCollision {
        /// Java scope where the collision was found.
        scope: String,
        /// Generated Java name used more than once.
        name: String,
    },
    /// A backend template failed to render.
    #[error("template rendering failed: {message}")]
    Template {
        /// Template rendering error message.
        message: String,
    },
}

impl From<askama::Error> for BackendError {
    fn from(error: askama::Error) -> Self {
        Self::Template {
            message: error.to_string(),
        }
    }
}
