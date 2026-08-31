use serde::{Deserialize, Serialize};

use crate::{
    DeprecationInfo, DocComment, EnumId, FieldDef, ReprAttr, Source, SourceName, SourceSpan,
    TypeExpr, UserAttr,
};

/// A Rust enum exported through BoltFFI.
///
/// An enum keeps the public shape of the Rust declaration: variants,
/// discriminants, payloads, representation hints, methods, and documentation.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EnumDef {
    /// Stable enum identity derived from the canonical Rust path.
    pub id: EnumId,
    /// Source enum name.
    pub name: SourceName,
    /// Variants written on the Rust enum.
    pub variants: Vec<VariantDef>,
    /// `repr` attributes written on the enum.
    pub repr: ReprAttr,
    /// User attributes preserved from the enum.
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the enum.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the enum.
    pub deprecated: Option<DeprecationInfo>,
    /// Methods attached to the enum.
    pub methods: Vec<crate::MethodDef>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Proc-macro span available while scanning the user crate.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl EnumDef {
    /// Builds an empty enum definition.
    ///
    /// The `id` parameter is the stable enum ID. The `name` parameter is the
    /// canonical source name.
    ///
    /// Returns an enum with no variants, attributes, or methods.
    pub fn new(id: EnumId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            variants: Vec::new(),
            repr: ReprAttr::none(),
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            methods: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }
}

/// A variant written inside an exported Rust enum.
///
/// Variants preserve the user-facing enum shape, including discriminants and
/// whether the payload was unit, tuple, or struct style.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct VariantDef {
    /// Source variant name.
    pub name: SourceName,
    /// Discriminant written in source, when one exists.
    pub discriminant: Option<i128>,
    /// Payload shape written by the variant.
    pub payload: VariantPayload,
    /// Whether the variant was marked `#[boltffi::transparent]`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub transparent: bool,
    /// Documentation attached to the variant.
    pub doc: Option<DocComment>,
    /// User attributes preserved from the variant.
    pub user_attrs: Vec<UserAttr>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Proc-macro span available while scanning the user crate.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl VariantDef {
    /// Builds a unit variant.
    ///
    /// The `name` parameter is the canonical variant name.
    ///
    /// Returns a variant with no payload and no discriminant.
    pub fn unit(name: impl Into<SourceName>) -> Self {
        Self {
            name: name.into(),
            discriminant: None,
            payload: VariantPayload::Unit,
            transparent: false,
            doc: None,
            user_attrs: Vec::new(),
            source: Source::exported(),
            source_span: None,
        }
    }
}

/// The payload shape written on an enum variant.
///
/// Tuple and struct variants both carry fields, but they are different public
/// APIs. The AST keeps that distinction explicit.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum VariantPayload {
    /// A variant without fields.
    Unit,
    /// A tuple variant such as `Value(u32, String)`.
    Tuple(Vec<TypeExpr>),
    /// A struct variant such as `Value { id: u32 }`.
    Struct(Vec<FieldDef>),
}
