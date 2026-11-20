use serde::{Deserialize, Serialize};

use crate::TypeExpr;
use crate::{
    ConstExpr, CustomTypeId, DeprecationInfo, DocComment, NamePart, Path, PathRoot, Source,
    SourceName, SourceSpan, UserAttr,
};

/// A user-declared custom type.
///
/// Custom types describe a Rust type that should be exposed through a different
/// representation type, together with the Rust functions that convert between
/// the two.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomTypeDef {
    /// Stable custom type identity derived from the canonical Rust path.
    pub id: CustomTypeId,
    /// Source custom type name.
    pub name: SourceName,
    /// Remote Rust type being represented.
    pub remote: CustomRemoteType,
    /// Rust source representation type used at the FFI surface.
    pub repr: TypeExpr,
    /// Error type returned by the fallible representation-to-remote converter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<CustomRemoteType>,
    /// Converter functions supplied by the source declaration.
    pub converters: CustomTypeConverters,
    /// User attributes preserved from the custom type declaration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the custom type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the custom type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    #[serde(default, skip_serializing_if = "Source::is_exported")]
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl CustomTypeDef {
    /// Builds a custom type definition.
    ///
    /// The `id` parameter is the stable custom type ID. The `name` parameter is
    /// the canonical source name. The `remote`, `repr`, `error`, and `converters`
    /// parameters record the user-declared conversion surface.
    ///
    /// Returns a custom type with no user attributes or documentation.
    pub fn new(
        id: CustomTypeId,
        name: impl Into<SourceName>,
        remote: CustomRemoteType,
        repr: TypeExpr,
        error: Option<CustomRemoteType>,
        converters: CustomTypeConverters,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            remote,
            repr,
            error,
            converters,
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }
}

/// A Rust type named by a custom type declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum CustomRemoteType {
    /// A named Rust type path.
    Path(CustomRemotePath),
    /// A Rust tuple type.
    Tuple(Vec<CustomRemoteType>),
}

impl CustomRemoteType {
    /// Builds a named Rust type path.
    pub fn path(path: CustomRemotePath) -> Self {
        Self::Path(path)
    }

    /// Builds a single-segment relative Rust type path.
    pub fn single_path(name: impl Into<NamePart>) -> Self {
        Self::Path(CustomRemotePath::single(name))
    }
}

/// A Rust type path named by a custom type declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomRemotePath {
    /// Where path resolution starts.
    pub root: PathRoot,
    /// Segments from root to leaf.
    pub segments: Vec<CustomRemotePathSegment>,
}

impl CustomRemotePath {
    /// Builds a Rust type path from its root qualifier and segments.
    pub fn new(root: PathRoot, segments: Vec<CustomRemotePathSegment>) -> Self {
        Self { root, segments }
    }

    /// Builds a relative Rust type path with a single segment.
    pub fn single(name: impl Into<NamePart>) -> Self {
        Self {
            root: PathRoot::Relative,
            segments: vec![CustomRemotePathSegment::new(name)],
        }
    }

    /// Returns the final segment, if the path has one.
    pub fn last(&self) -> Option<&CustomRemotePathSegment> {
        self.segments.last()
    }
}

/// One segment of a custom remote Rust type path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomRemotePathSegment {
    /// The canonical spelling of this path segment.
    pub name: NamePart,
    /// Generic arguments attached to this segment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<CustomRemoteGenericArgument>,
}

impl CustomRemotePathSegment {
    /// Builds a path segment without generic arguments.
    pub fn new(name: impl Into<NamePart>) -> Self {
        Self {
            name: name.into(),
            arguments: Vec::new(),
        }
    }

    /// Builds a path segment with explicit generic arguments.
    pub fn with_arguments(
        name: impl Into<NamePart>,
        arguments: Vec<CustomRemoteGenericArgument>,
    ) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

/// A generic argument in a custom remote Rust type path.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum CustomRemoteGenericArgument {
    /// A type argument.
    Type(Box<CustomRemoteType>),
    /// A const argument.
    Const(ConstExpr),
    /// An associated type equality.
    AssociatedType {
        /// The associated type being assigned.
        name: NamePart,
        /// The type written on the right side of the equality.
        ty: Box<CustomRemoteType>,
    },
}

/// Converter functions attached to a custom type declaration.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomTypeConverters {
    /// Converter that turns the remote Rust value into the representation type.
    pub into_ffi: CustomTypeConverter,
    /// Converter that rebuilds the remote Rust value from the representation type.
    pub try_from_ffi: CustomTypeConverter,
}

impl CustomTypeConverters {
    /// Builds a pair of custom type converters.
    pub fn new(into_ffi: CustomTypeConverter, try_from_ffi: CustomTypeConverter) -> Self {
        Self {
            into_ffi,
            try_from_ffi,
        }
    }
}

/// A Rust expression used as a custom type converter.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum CustomTypeConverter {
    /// A converter named by a Rust path.
    Path(Path),
    /// A `CustomFfiConvertible` method on the remote Rust type.
    TraitMethod(CustomTraitMethodConverter),
    /// A converter written as an inline Rust expression.
    Expr(CustomConverterExpr),
}

impl CustomTypeConverter {
    /// Builds a path converter.
    pub fn path(path: Path) -> Self {
        Self::Path(path)
    }

    /// Builds a `CustomFfiConvertible` method converter.
    pub fn trait_method(receiver: Path, method: impl Into<NamePart>) -> Self {
        Self::TraitMethod(CustomTraitMethodConverter::new(receiver, method))
    }

    /// Builds an inline expression converter.
    pub fn expr(source: impl Into<String>) -> Self {
        Self::Expr(CustomConverterExpr::new(source))
    }
}

/// A converter method selected from `CustomFfiConvertible`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomTraitMethodConverter {
    /// Remote Rust type implementing `CustomFfiConvertible`.
    pub receiver: Path,
    /// Converter method name.
    pub method: NamePart,
}

impl CustomTraitMethodConverter {
    /// Builds a trait-method converter.
    pub fn new(receiver: Path, method: impl Into<NamePart>) -> Self {
        Self {
            receiver,
            method: method.into(),
        }
    }
}

/// Source text for an inline custom converter expression.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomConverterExpr {
    /// Rust expression source.
    pub source: String,
}

impl CustomConverterExpr {
    /// Builds an inline converter expression from Rust source text.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }
}
