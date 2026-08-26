//! Canonical source AST for exported BoltFFI APIs.
//!
//! The values in this crate are the handoff from macro scanning to resolution.
//! They describe the exported Rust surface in a stable, serializable vocabulary:
//! records, enums, functions, classes, callbacks, streams, constants, custom
//! types, attributes, documentation, and type expressions.
//!
//! A record here can carry `#[repr(C)]`, named fields, and methods. The crate
//! stops at that source description; resolved ABI and binding shapes live in
//! the next pipeline stages.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod attribute;
mod callable;
mod class;
mod constant;
mod contract;
mod custom;
mod documentation;
mod enumeration;
mod ids;
mod literal;
mod name;
mod primitive;
mod record;
mod source;
mod stream;
mod trait_def;
mod type_expr;

pub use attribute::{AttributeInput, ReprAttr, ReprItem, UserAttr};
pub use callable::{
    CallableForm, ExecutionKind, FunctionDef, MethodDef, ParameterDef, ParameterPassing, Receiver,
    ReturnDef,
};
pub use class::{ClassDef, ClassThreadSafety};
pub use constant::{ConstantDef, ConstantOwner};
pub use contract::{PackageInfo, SourceContract};
pub use custom::{
    CustomConverterExpr, CustomRemoteGenericArgument, CustomRemotePath, CustomRemotePathSegment,
    CustomRemoteType, CustomTypeConverter, CustomTypeConverters, CustomTypeDef,
};
pub use documentation::{DeprecationInfo, DocComment};
pub use enumeration::{EnumDef, VariantDef, VariantPayload};
pub use ids::{
    ClassId, ConstantId, CustomTypeId, DeclarationId, EnumId, FunctionId, MethodId, RecordId,
    StreamId, TraitId,
};
pub use literal::{ConstExpr, DefaultValue, FloatLiteral, IntegerLiteral, Literal};
pub use name::{CanonicalName, GenericArgument, NamePart, Path, PathRoot, PathSegment, SourceName};
pub use primitive::Primitive;
pub use record::{FieldDef, RecordDef, RecordEncoding};
pub use source::{Source, SourceFile, SourceSpan, Visibility};
pub use stream::{StreamDef, StreamMode};
pub use trait_def::TraitDef;
pub use type_expr::{
    AdditionalBound, BaseTrait, BuiltinType, FnSig, FnTrait, FnTraitKind, MapKind, TraitBounds,
    TypeExpr, TypeParameter,
};
