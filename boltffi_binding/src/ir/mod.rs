//! What is in `Bindings<S>`, and how a consumer reads it.
//!
//! When `#[data]` and `#[export]` see the user's source, the bind
//! pass walks every exported item against a target [`Surface`]: this
//! record is bytes that can cross by memcpy, that enum has a payload
//! that needs encoding, this async function returns a poll handle. By
//! the time a [`Bindings`] reaches a consumer, the decisions are over.
//! Every declaration carries its boundary plan attached, and the
//! choices that diverge between targets (callback dispatch, buffer
//! layout, handle carrier, async protocol) are picked once for the
//! surface the contract is parameterized by.
//!
//! Generating Swift, Kotlin, Python, or any other target language is
//! not in this module. The work here ends at the resolved facts.
//!
//! # The shape of a contract
//!
//! For the source
//!
//! ```ignore
//! use boltffi::*;
//!
//! #[data]
//! pub struct Point { pub x: f64, pub y: f64 }
//!
//! #[export]
//! pub fn distance(a: Point, b: Point) -> f64 { /* ... */ }
//! ```
//!
//! `Point` becomes a [`RecordDecl::Direct`]. Both fields are primitives
//! with predictable layout, so the bind pass picks the direct path: 16
//! bytes total, 8-byte alignment, `x` at offset 0, `y` at offset 8.
//! Foreign code reads the struct by offset. With a `String` field, the
//! same source would have produced a [`RecordDecl::Encoded`] instead,
//! carrying a [`ReadPlan`] and a [`WritePlan`] for moving the bytes.
//!
//! `distance` becomes a [`FunctionDecl`]. Inside it, a [`CallableDecl`]
//! holds the receiver mode, two [`ParamDecl`]s that lower as direct
//! `Point` values, and a primitive `f64` return. The native symbol
//! foreign code calls (`demo_distance` on native, the same identifier
//! at the wasm export on wasm32) lives on the surrounding
//! `FunctionDecl`. Synchronous. No error path.
//!
//! Both refer back to a [`NativeSymbolTable`] hanging off the
//! `Bindings<S>` value, alongside a [`PackageInfo`] used in
//! diagnostics.
//!
//! # Consuming a contract
//!
//! Pattern match on [`Decl<S>`]:
//!
//! ```ignore
//! for decl in bindings.decls() {
//!     match decl {
//!         Decl::Record(record) => render_record(record),
//!         Decl::Function(function) => render_function(function),
//!         _ => continue,
//!     }
//! }
//! ```
//!
//! Validation runs before the value reaches a consumer. Inside a match
//! arm, every id is unique inside its family, every native symbol is
//! callable, and the schema version is one this code understands. No
//! fallible accessor exists; a held [`Bindings`] is consistent, or
//! construction would have failed.
//!
//! [`Decl`] is the front door. [`RecordDecl`], [`EnumDecl`],
//! [`CallableDecl`], and [`CodecNode`] are where most of the real shape
//! lives. [`Surface`], [`Native`], and [`Wasm32`] gate the target
//! divergence.

#![allow(dead_code)]

mod callable;
mod closure;
mod codec;
mod contract;
mod custom;
mod decl;
mod direction;
mod error;
mod error_payloads;
mod ids;
mod imports;
mod layout;
mod metadata;
mod name;
mod op;
mod primitive;
mod reference;
mod surface;
mod symbol;
mod types;

pub use boltffi_ast::{BuiltinType, MapKind};
pub use callable::{
    CallableDecl, ClosureForm, ClosureParameter, ClosureRegistration, ClosureReturn, ErrorChannel,
    ErrorDecl, ErrorPlacement, ExecutionDecl, ExportedCallable, ImportedCallable, IncomingParam,
    OutgoingParam, ParamDecl, ParamDirection, ParamPlan, ParamPlanRender, Receive, ReturnDecl,
    ReturnPlan, ReturnPlanRender, ReturnValueSlot,
};
pub use closure::ClosureSignature;
pub use codec::{
    CodecNode, CodecPlan, CodecRead, CodecSize, CodecWrite, OwnedWireEncoding, ReadPlan, WritePlan,
};
pub use contract::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BINDING_METADATA_BUILD_ENV, BINDING_METADATA_FEATURES_ENV,
    BINDING_METADATA_ROOT_ENV, BINDING_METADATA_SOURCE_ENV, BINDING_METADATA_SURFACE_ENV,
    BindingMetadataEnvelope, BindingMetadataError, BindingMetadataFormat, BindingMetadataHash,
    BindingMetadataSection, BindingMetadataSectionBytes, BindingMetadataSurface, Bindings,
    ContractVersion, PackageInfo, SerializedBindings,
};
pub use custom::{
    CustomConverterExpression, CustomConverterPath, CustomConverterPathRoot, CustomTypeConverter,
    CustomTypeConverters,
};
pub use decl::{
    CStyleEnumDecl, CStyleVariantDecl, CallbackDecl, CallbackLocalFunction,
    CallbackLocalMethodDecl, CallbackLocalProtocol, ClassDecl, ClassThreadSafety, ConstantDecl,
    ConstantValueDecl, CustomTypeDecl, DataEnumDecl, DataVariantDecl, DataVariantPayload, Decl,
    DeclarationRef, DeclarationRole, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl,
    EncodedRecordDecl, EnumDecl, ExportedMethodDecl, FieldKey, FunctionDecl, ImportedMethodDecl,
    InitializerDecl, MethodDecl, RecordDecl, StreamDecl, StreamItemPlan, StreamItemPlanRender,
    StreamMode, StreamProtocol, VariantTag,
};
pub(crate) use decl::{ClassDeclParts, InvalidClassDecl, StreamDeclParts};
pub use direction::{CallableScope, Direction, ForeignBody, IntoRust, OutOfRust, RustBody};
pub use error::{BindingError, BindingErrorKind};
pub use ids::{
    CallbackId, ClassId, ConstantId, CustomTypeId, DeclarationId, EnumId, FunctionId,
    InitializerId, MethodId, RecordId, StreamId, SymbolId,
};
pub use imports::{WasmImports, WasmIncomingClosure};
pub use layout::{AlignmentError, ByteAlignment, ByteOffset, ByteSize, FieldLayout, RecordLayout};
pub use metadata::{
    DeclMeta, DefaultValue, DeprecationInfo, DocComment, ElementMeta, FloatValue, IntegerValue,
};
pub use name::{CanonicalName, NamePart};
pub use op::{
    BinderId, ByteCount, ElementCount, IntrinsicOp, Op, OpNode, OpRender, Scalar, ScalarTy, Truth,
    ValueRef, ValueRoot,
};
pub use primitive::{IntegerRepr, Primitive};
pub use reference::DeclarationShape;
pub use surface::{
    AsyncProtocolIntrospect, BufferShapeRules, CallbackProtocolIntrospect,
    ClosureRegistrationIntrospect, Native, Surface, Wasm32, native, wasm32,
};
pub use symbol::{
    ImportModule, ImportSymbol, NativeSymbol, NativeSymbolTable, SymbolName, VTableSlot,
};
pub use types::{
    DirectFieldType, DirectValueType, DirectVectorElementType, DirectVectorPrimitive,
    HandlePresence, HandleTarget, ReturnTypeRef, TypeRef, TypeRefRender,
};
