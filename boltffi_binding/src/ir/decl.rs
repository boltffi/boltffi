use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::{
    BufferShapeRules, BuiltinType, ByteSize, CallableScope, CallbackId, CallbackProtocolIntrospect,
    CanonicalName, ClassId, CodecPlan, ConstantId, CustomTypeConverters, CustomTypeId, DeclMeta,
    DeclarationId, DefaultValue, DirectFieldType, DirectValueType, ElementMeta, EnumId,
    ExportedCallable, FunctionId, ImportedCallable, InitializerId, IntegerRepr, IntegerValue,
    MethodId, NamePart, NativeSymbol, ReadPlan, Receive, RecordId, RecordLayout, ReturnTypeRef,
    RustBody, StreamId, Surface, TypeRef, WritePlan,
};

/// One classified declaration in a binding contract.
///
/// The variants enumerate every kind of FFI-exported item the contract can
/// describe. Each variant carries a fully resolved declaration: matching
/// on the variant and then on the inner shape yields a value with every
/// FFI decision already made.
///
/// Generic over `S: Surface` because every variant transitively contains
/// at least one [`CallableDecl`], and callable shapes diverge by target.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, S::CallbackProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, S::CallbackProtocol: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum Decl<S: Surface> {
    /// Record declaration.
    Record(Box<RecordDecl<S>>),
    /// Enum declaration.
    Enum(Box<EnumDecl<S>>),
    /// Free function declaration.
    Function(Box<FunctionDecl<S>>),
    /// Class-style object declaration.
    Class(Box<ClassDecl<S>>),
    /// Callback trait declaration.
    Callback(Box<CallbackDecl<S>>),
    /// Stream declaration.
    Stream(Box<StreamDecl<S>>),
    /// Constant declaration.
    Constant(Box<ConstantDecl<S>>),
    /// Custom type declaration.
    CustomType(Box<CustomTypeDecl>),
}

/// Borrowed view of a classified declaration.
///
/// This view lists the declaration families a renderer must handle. Unlike
/// [`Decl`], it is intentionally exhaustive: adding a declaration family must
/// update every consumer that pattern-matches this type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeclarationRef<'a, S: Surface> {
    /// Record declaration.
    Record(&'a RecordDecl<S>),
    /// Enum declaration.
    Enum(&'a EnumDecl<S>),
    /// Free function declaration.
    Function(&'a FunctionDecl<S>),
    /// Class-style object declaration.
    Class(&'a ClassDecl<S>),
    /// Callback trait declaration.
    Callback(&'a CallbackDecl<S>),
    /// Stream declaration.
    Stream(&'a StreamDecl<S>),
    /// Constant declaration.
    Constant(&'a ConstantDecl<S>),
    /// Custom type declaration.
    CustomType(&'a CustomTypeDecl),
}

/// The role a declaration plays in the lowered contract.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeclarationRole {
    /// A declaration exported as part of the user-facing API.
    #[default]
    Value,
    /// A declaration carried by an encoded error channel.
    ErrorPayload,
}

impl DeclarationRole {
    const fn is_value(&self) -> bool {
        matches!(self, Self::Value)
    }

    /// Returns whether the declaration is carried as an encoded error payload.
    pub const fn is_error_payload(&self) -> bool {
        matches!(self, Self::ErrorPayload)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl<'a, S: Surface> From<&'a Decl<S>> for DeclarationRef<'a, S> {
    fn from(decl: &'a Decl<S>) -> Self {
        match decl {
            Decl::Record(record) => Self::Record(record.as_ref()),
            Decl::Enum(enum_decl) => Self::Enum(enum_decl.as_ref()),
            Decl::Function(function) => Self::Function(function.as_ref()),
            Decl::Class(class) => Self::Class(class.as_ref()),
            Decl::Callback(callback) => Self::Callback(callback.as_ref()),
            Decl::Stream(stream) => Self::Stream(stream.as_ref()),
            Decl::Constant(constant) => Self::Constant(constant.as_ref()),
            Decl::CustomType(custom) => Self::CustomType(custom.as_ref()),
        }
    }
}

impl<'a, S: Surface> DeclarationRef<'a, S> {
    /// Returns the record declaration when this view is a record.
    pub const fn record(self) -> Option<&'a RecordDecl<S>> {
        match self {
            Self::Record(record) => Some(record),
            _ => None,
        }
    }

    /// Returns the enum declaration when this view is an enum.
    pub const fn enumeration(self) -> Option<&'a EnumDecl<S>> {
        match self {
            Self::Enum(enumeration) => Some(enumeration),
            _ => None,
        }
    }

    /// Returns the function declaration when this view is a function.
    pub const fn function(self) -> Option<&'a FunctionDecl<S>> {
        match self {
            Self::Function(function) => Some(function),
            _ => None,
        }
    }

    /// Returns the class declaration when this view is a class.
    pub const fn class(self) -> Option<&'a ClassDecl<S>> {
        match self {
            Self::Class(class) => Some(class),
            _ => None,
        }
    }

    /// Returns the callback declaration when this view is a callback.
    pub const fn callback(self) -> Option<&'a CallbackDecl<S>> {
        match self {
            Self::Callback(callback) => Some(callback),
            _ => None,
        }
    }

    /// Returns the stream declaration when this view is a stream.
    pub const fn stream(self) -> Option<&'a StreamDecl<S>> {
        match self {
            Self::Stream(stream) => Some(stream),
            _ => None,
        }
    }

    /// Returns the constant declaration when this view is a constant.
    pub const fn constant(self) -> Option<&'a ConstantDecl<S>> {
        match self {
            Self::Constant(constant) => Some(constant),
            _ => None,
        }
    }

    /// Returns the custom type declaration when this view is a custom type.
    pub const fn custom_type(self) -> Option<&'a CustomTypeDecl> {
        match self {
            Self::CustomType(custom_type) => Some(custom_type),
            _ => None,
        }
    }

    /// Returns whether any encoded value in this declaration uses a result codec.
    pub fn uses_result_codec(self) -> bool {
        match self {
            Self::Record(record) => record.uses_result_codec(),
            Self::Enum(enumeration) => enumeration.uses_result_codec(),
            Self::Function(function) => function.uses_result_codec(),
            Self::Class(class) => class.uses_result_codec(),
            Self::Callback(callback) => callback.uses_result_codec(),
            Self::Stream(stream) => stream.uses_result_codec(),
            Self::Constant(constant) => constant.uses_result_codec(),
            Self::CustomType(_) => false,
        }
    }

    /// Returns whether any encoded value in this declaration uses the given builtin codec.
    pub fn uses_builtin_codec(self, kind: BuiltinType) -> bool {
        match self {
            Self::Record(record) => record.uses_builtin_codec(kind),
            Self::Enum(enumeration) => enumeration.uses_builtin_codec(kind),
            Self::Function(function) => function.uses_builtin_codec(kind),
            Self::Class(class) => class.uses_builtin_codec(kind),
            Self::Callback(callback) => callback.uses_builtin_codec(kind),
            Self::Stream(stream) => stream.uses_builtin_codec(kind),
            Self::Constant(constant) => constant.uses_builtin_codec(kind),
            Self::CustomType(_) => false,
        }
    }

    /// Returns whether any value crossing in this declaration uses a direct record vector.
    pub fn uses_direct_record_vector(self) -> bool {
        match self {
            Self::Record(record) => record.uses_direct_record_vector(),
            Self::Enum(enumeration) => enumeration.uses_direct_record_vector(),
            Self::Function(function) => function.uses_direct_record_vector(),
            Self::Class(class) => class.uses_direct_record_vector(),
            Self::Callback(callback) => callback.uses_direct_record_vector(),
            Self::Stream(stream) => stream.uses_direct_record_vector(),
            Self::Constant(constant) => constant.uses_direct_record_vector(),
            Self::CustomType(_) => false,
        }
    }

    /// Returns whether any callable in this declaration uses an asynchronous execution protocol.
    pub fn uses_async_execution(self) -> bool {
        match self {
            Self::Record(record) => record.uses_async_execution(),
            Self::Enum(enumeration) => enumeration.uses_async_execution(),
            Self::Function(function) => function.uses_async_execution(),
            Self::Class(class) => class.uses_async_execution(),
            Self::Callback(callback) => callback.uses_async_execution(),
            Self::Stream(_) | Self::Constant(_) | Self::CustomType(_) => false,
        }
    }
}

impl<S: Surface> Decl<S> {
    /// Returns the typed identity of this declaration.
    pub fn id(&self) -> DeclarationId {
        match self {
            Self::Record(record) => DeclarationId::Record(record.id()),
            Self::Enum(enum_decl) => DeclarationId::Enum(enum_decl.id()),
            Self::Function(function) => DeclarationId::Function(function.id()),
            Self::Class(class) => DeclarationId::Class(class.id()),
            Self::Callback(callback) => DeclarationId::Callback(callback.id()),
            Self::Stream(stream) => DeclarationId::Stream(stream.id()),
            Self::Constant(constant) => DeclarationId::Constant(constant.id()),
            Self::CustomType(custom) => DeclarationId::CustomType(custom.id()),
        }
    }

    /// Iterates over every Rust-implemented callable this declaration
    /// owns.
    ///
    /// Records, enums, and classes yield their initializers and
    /// methods. A function yields its single callable. A constant
    /// yields the accessor's callable when it has one. Callback
    /// declarations yield Rust-side local protocol methods when the
    /// callback can be implemented in Rust.
    pub fn exported_callables(&self) -> Box<dyn Iterator<Item = &ExportedCallable<S>> + '_> {
        match self {
            Self::Record(record) => Box::new(
                record
                    .initializers()
                    .iter()
                    .map(|initializer| initializer.callable())
                    .chain(record.methods().iter().map(|method| method.callable())),
            ),
            Self::Enum(enumeration) => Box::new(
                enumeration
                    .initializers()
                    .iter()
                    .map(|initializer| initializer.callable())
                    .chain(enumeration.methods().iter().map(|method| method.callable())),
            ),
            Self::Function(function) => Box::new(std::iter::once(function.callable())),
            Self::Class(class) => Box::new(
                class
                    .initializers()
                    .iter()
                    .map(|initializer| initializer.callable())
                    .chain(class.methods().iter().map(|method| method.callable())),
            ),
            Self::Constant(constant) => match constant.value() {
                ConstantValueDecl::Inline { .. } => Box::new(std::iter::empty()),
                ConstantValueDecl::Accessor { callable, .. } => {
                    Box::new(std::iter::once(callable.as_ref()))
                }
            },
            Self::Callback(callback) => match callback.local_protocol() {
                Some(protocol) => {
                    Box::new(protocol.methods().iter().map(|method| method.callable()))
                }
                None => Box::new(std::iter::empty()),
            },
            Self::Stream(_) | Self::CustomType(_) => Box::new(std::iter::empty()),
        }
    }

    /// Iterates over every foreign-implemented callable this
    /// declaration owns.
    ///
    /// Callback declarations yield one entry per method the protocol
    /// exposes. Every other declaration kind yields nothing.
    pub fn imported_callables(&self) -> Box<dyn Iterator<Item = &ImportedCallable<S>> + '_> {
        match self {
            Self::Callback(callback) => callback.protocol().method_callables(),
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Iterates over every native symbol this declaration references.
    ///
    /// Combines the declaration's own symbols (function/initializer
    /// symbols, class release, callback registration, stream protocol,
    /// constant accessor) with the symbols every nested callable
    /// references through its async protocol.
    pub fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        let nested = self
            .exported_callables()
            .flat_map(ExportedCallable::native_symbols)
            .chain(
                self.imported_callables()
                    .flat_map(ImportedCallable::native_symbols),
            );
        match self {
            Self::Record(record) => Box::new(
                record
                    .initializers()
                    .iter()
                    .map(|initializer| initializer.symbol())
                    .chain(record.methods().iter().map(|method| method.target()))
                    .chain(nested),
            ),
            Self::Enum(enumeration) => Box::new(
                enumeration
                    .initializers()
                    .iter()
                    .map(|initializer| initializer.symbol())
                    .chain(enumeration.methods().iter().map(|method| method.target()))
                    .chain(nested),
            ),
            Self::Function(function) => Box::new(std::iter::once(function.symbol()).chain(nested)),
            Self::Class(class) => Box::new(
                std::iter::once(class.release())
                    .chain(
                        class
                            .initializers()
                            .iter()
                            .map(|initializer| initializer.symbol()),
                    )
                    .chain(class.methods().iter().map(|method| method.target()))
                    .chain(nested),
            ),
            Self::Callback(callback) => {
                Box::new(callback.protocol().native_symbols().chain(nested))
            }
            Self::Stream(stream) => Box::new(
                [
                    stream.protocol().subscribe(),
                    stream.protocol().pop_batch(),
                    stream.protocol().wait(),
                    stream.protocol().poll(),
                    stream.protocol().unsubscribe(),
                    stream.protocol().free(),
                ]
                .into_iter()
                .chain(nested),
            ),
            Self::Constant(constant) => match constant.value() {
                ConstantValueDecl::Inline { .. } => Box::new(nested),
                ConstantValueDecl::Accessor { symbol, .. } => {
                    Box::new(std::iter::once(symbol).chain(nested))
                }
            },
            Self::CustomType(_) => Box::new(nested),
        }
    }
}

/// A user-defined record after the classifier chose how it crosses.
///
/// `Direct` means the record's bytes themselves are the wire shape and
/// foreign code reads them by offset. `Encoded` means the record is
/// serialized into the contract's wire format and reconstructed on the
/// other side.
///
/// # Example
///
/// `struct Point { x: f64, y: f64 }` typically classifies as `Direct`
/// because both halves are primitives with predictable layout.
/// `struct UserProfile { name: String, friends: Vec<UserProfile> }`
/// classifies as `Encoded`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum RecordDecl<S: Surface> {
    /// Crosses by raw memory.
    Direct(Box<DirectRecordDecl<S>>),
    /// Crosses through encoded bytes.
    Encoded(Box<EncodedRecordDecl<S>>),
}

impl<S: Surface> RecordDecl<S> {
    pub(crate) fn direct(record: DirectRecordDecl<S>) -> Self {
        Self::Direct(Box::new(record))
    }

    pub(crate) fn encoded(record: EncodedRecordDecl<S>) -> Self {
        Self::Encoded(Box::new(record))
    }

    /// Returns the record id.
    pub const fn id(&self) -> RecordId {
        match self {
            Self::Direct(record) => record.id(),
            Self::Encoded(record) => record.id(),
        }
    }

    /// Returns the canonical record name.
    pub fn name(&self) -> &CanonicalName {
        match self {
            Self::Direct(record) => record.name(),
            Self::Encoded(record) => record.name(),
        }
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        match self {
            Self::Direct(record) => record.meta(),
            Self::Encoded(record) => record.meta(),
        }
    }

    /// Returns the record initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        match self {
            Self::Direct(record) => record.initializers(),
            Self::Encoded(record) => record.initializers(),
        }
    }

    /// Returns the record methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        match self {
            Self::Direct(record) => record.methods(),
            Self::Encoded(record) => record.methods(),
        }
    }

    /// Returns the declaration role.
    pub const fn role(&self) -> DeclarationRole {
        match self {
            Self::Direct(record) => record.role(),
            Self::Encoded(record) => record.role(),
        }
    }

    /// Returns whether this record is carried by an encoded error channel.
    pub const fn is_error_payload(&self) -> bool {
        self.role().is_error_payload()
    }

    /// Returns whether this record is referenced by an encoded codec plan.
    pub const fn is_codec_payload(&self) -> bool {
        match self {
            Self::Direct(record) => record.is_codec_payload(),
            Self::Encoded(_) => true,
        }
    }

    pub(crate) fn mark_error_payload(&mut self) {
        match self {
            Self::Direct(record) => record.mark_error_payload(),
            Self::Encoded(record) => record.mark_error_payload(),
        }
    }

    pub(crate) fn mark_codec_payload(&mut self) {
        if let Self::Direct(record) = self {
            record.mark_codec_payload();
        }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::Direct(record) => record.uses_result_codec(),
            Self::Encoded(record) => record.uses_result_codec(),
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Direct(record) => record.uses_builtin_codec(kind),
            Self::Encoded(record) => record.uses_builtin_codec(kind),
        }
    }

    fn uses_direct_record_vector(&self) -> bool {
        match self {
            Self::Direct(record) => record.uses_direct_record_vector(),
            Self::Encoded(record) => record.uses_direct_record_vector(),
        }
    }

    fn uses_async_execution(&self) -> bool {
        match self {
            Self::Direct(record) => record.uses_async_execution(),
            Self::Encoded(record) => record.uses_async_execution(),
        }
    }
}

/// A record that crosses the boundary as raw memory.
///
/// Carries the byte-level [`RecordLayout`] alongside its fields so
/// foreign code reads each field at the agreed-upon offset rather than
/// asking Rust to serialize on every call.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct DirectRecordDecl<S: Surface> {
    id: RecordId,
    name: CanonicalName,
    #[serde(default, skip_serializing_if = "DeclarationRole::is_value")]
    role: DeclarationRole,
    #[serde(default, skip_serializing_if = "is_false")]
    codec_payload: bool,
    meta: DeclMeta,
    fields: Vec<DirectFieldDecl>,
    initializers: Vec<InitializerDecl<S>>,
    methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
    layout: RecordLayout,
}

impl<S: Surface> DirectRecordDecl<S> {
    pub(crate) fn new(
        id: RecordId,
        name: CanonicalName,
        meta: DeclMeta,
        fields: Vec<DirectFieldDecl>,
        initializers: Vec<InitializerDecl<S>>,
        methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
        layout: RecordLayout,
    ) -> Self {
        Self {
            id,
            name,
            role: DeclarationRole::Value,
            codec_payload: false,
            meta,
            fields,
            initializers,
            methods,
            layout,
        }
    }

    /// Returns the record id.
    pub const fn id(&self) -> RecordId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration role.
    pub const fn role(&self) -> DeclarationRole {
        self.role
    }

    /// Returns whether this declaration is carried by an encoded error channel.
    pub const fn is_error_payload(&self) -> bool {
        self.role.is_error_payload()
    }

    /// Returns whether this direct record is referenced by an encoded codec plan.
    pub const fn is_codec_payload(&self) -> bool {
        self.codec_payload
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the fields in source order.
    pub fn fields(&self) -> &[DirectFieldDecl] {
        &self.fields
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        &self.methods
    }

    /// Returns the byte-level layout.
    pub fn layout(&self) -> &RecordLayout {
        &self.layout
    }

    fn uses_result_codec(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_result_codec)
            || self.methods.iter().any(MethodDecl::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.initializers
            .iter()
            .any(|initializer| initializer.uses_builtin_codec(kind))
            || self
                .methods
                .iter()
                .any(|method| method.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_direct_record_vector)
            || self
                .methods
                .iter()
                .any(MethodDecl::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_async_execution)
            || self.methods.iter().any(MethodDecl::uses_async_execution)
    }

    fn mark_error_payload(&mut self) {
        self.role = DeclarationRole::ErrorPayload;
    }

    fn mark_codec_payload(&mut self) {
        self.codec_payload = true;
    }
}

/// A record that crosses the boundary through encoded bytes.
///
/// Each field carries its own per-field codec, and the record itself
/// carries a [`CodecPlan`] for moving the whole value in either
/// direction.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct EncodedRecordDecl<S: Surface> {
    id: RecordId,
    name: CanonicalName,
    #[serde(default, skip_serializing_if = "DeclarationRole::is_value")]
    role: DeclarationRole,
    meta: DeclMeta,
    fields: Vec<EncodedFieldDecl>,
    initializers: Vec<InitializerDecl<S>>,
    methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
    codec: CodecPlan,
}

impl<S: Surface> EncodedRecordDecl<S> {
    pub(crate) fn new(
        id: RecordId,
        name: CanonicalName,
        meta: DeclMeta,
        fields: Vec<EncodedFieldDecl>,
        initializers: Vec<InitializerDecl<S>>,
        methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
        codec: CodecPlan,
    ) -> Self {
        Self {
            id,
            name,
            role: DeclarationRole::Value,
            meta,
            fields,
            initializers,
            methods,
            codec,
        }
    }

    /// Returns the record id.
    pub const fn id(&self) -> RecordId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration role.
    pub const fn role(&self) -> DeclarationRole {
        self.role
    }

    /// Returns whether this declaration is carried by an encoded error channel.
    pub const fn is_error_payload(&self) -> bool {
        self.role.is_error_payload()
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the fields in source order.
    pub fn fields(&self) -> &[EncodedFieldDecl] {
        &self.fields
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        &self.methods
    }

    /// Returns the whole-record read plan.
    pub fn read(&self) -> &ReadPlan {
        self.codec.read()
    }

    /// Returns the whole-record write plan.
    pub fn write(&self) -> &WritePlan {
        self.codec.write()
    }

    /// Returns the whole-record codec.
    pub fn codec(&self) -> &CodecPlan {
        &self.codec
    }

    fn uses_result_codec(&self) -> bool {
        self.fields.iter().any(EncodedFieldDecl::uses_result_codec)
            || self
                .initializers
                .iter()
                .any(InitializerDecl::uses_result_codec)
            || self.methods.iter().any(MethodDecl::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.fields
            .iter()
            .any(|field| field.uses_builtin_codec(kind))
            || self
                .initializers
                .iter()
                .any(|initializer| initializer.uses_builtin_codec(kind))
            || self
                .methods
                .iter()
                .any(|method| method.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_direct_record_vector)
            || self
                .methods
                .iter()
                .any(MethodDecl::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_async_execution)
            || self.methods.iter().any(MethodDecl::uses_async_execution)
    }

    fn mark_error_payload(&mut self) {
        self.role = DeclarationRole::ErrorPayload;
    }
}

/// How a field is named inside a record or variant payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldKey {
    /// Named field.
    Named(CanonicalName),
    /// Tuple field at the given zero-based position.
    Position(u32),
}

impl FieldKey {
    pub(crate) fn position(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self::Position)
    }
}

/// One field of a direct record.
///
/// Field offsets live on the parent record's [`RecordLayout`] rather
/// than on the field itself, so the layout can be validated as one
/// coherent value before any consumer reads from it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DirectFieldDecl {
    key: FieldKey,
    ty: DirectFieldType,
    meta: ElementMeta,
}

impl DirectFieldDecl {
    pub(crate) fn new(key: FieldKey, ty: DirectFieldType, meta: ElementMeta) -> Self {
        Self { key, ty, meta }
    }

    /// Returns the field key.
    pub fn key(&self) -> &FieldKey {
        &self.key
    }

    /// Returns the field type.
    pub fn ty(&self) -> DirectFieldType {
        self.ty
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }
}

/// One field of an encoded record or data enum payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EncodedFieldDecl {
    key: FieldKey,
    ty: TypeRef,
    codec: CodecPlan,
    meta: ElementMeta,
}

impl EncodedFieldDecl {
    pub(crate) fn new(key: FieldKey, ty: TypeRef, codec: CodecPlan, meta: ElementMeta) -> Self {
        Self {
            key,
            ty,
            codec,
            meta,
        }
    }

    /// Returns the field key.
    pub fn key(&self) -> &FieldKey {
        &self.key
    }

    /// Returns the field type.
    pub fn ty(&self) -> &TypeRef {
        &self.ty
    }

    /// Returns the read plan.
    pub fn read(&self) -> &ReadPlan {
        self.codec.read()
    }

    /// Returns the write plan.
    pub fn write(&self) -> &WritePlan {
        self.codec.write()
    }

    /// Returns the field codec.
    pub fn codec(&self) -> &CodecPlan {
        &self.codec
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    fn uses_result_codec(&self) -> bool {
        self.codec.uses_result()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.codec.uses_builtin(kind)
    }
}

/// A user-defined enum after the classifier chose how it crosses.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum EnumDecl<S: Surface> {
    /// Fieldless enum represented by an integer discriminant.
    CStyle(CStyleEnumDecl<S>),
    /// Payload-carrying enum represented by an encoded tag and payload.
    Data(Box<DataEnumDecl<S>>),
}

impl<S: Surface> EnumDecl<S> {
    pub(crate) fn c_style(enumeration: CStyleEnumDecl<S>) -> Self {
        Self::CStyle(enumeration)
    }

    pub(crate) fn data(enumeration: DataEnumDecl<S>) -> Self {
        Self::Data(Box::new(enumeration))
    }

    /// Returns the enum id.
    pub const fn id(&self) -> EnumId {
        match self {
            Self::CStyle(enumeration) => enumeration.id(),
            Self::Data(enumeration) => enumeration.id(),
        }
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        match self {
            Self::CStyle(enumeration) => enumeration.name(),
            Self::Data(enumeration) => enumeration.name(),
        }
    }

    /// Returns the enum initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        match self {
            Self::CStyle(enumeration) => enumeration.initializers(),
            Self::Data(enumeration) => enumeration.initializers(),
        }
    }

    /// Returns the enum methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        match self {
            Self::CStyle(enumeration) => enumeration.methods(),
            Self::Data(enumeration) => enumeration.methods(),
        }
    }

    /// Returns the declaration role.
    pub const fn role(&self) -> DeclarationRole {
        match self {
            Self::CStyle(enumeration) => enumeration.role(),
            Self::Data(enumeration) => enumeration.role(),
        }
    }

    /// Returns whether this enum is carried by an encoded error channel.
    pub const fn is_error_payload(&self) -> bool {
        self.role().is_error_payload()
    }

    pub(crate) fn mark_error_payload(&mut self) {
        match self {
            Self::CStyle(enumeration) => enumeration.mark_error_payload(),
            Self::Data(enumeration) => enumeration.mark_error_payload(),
        }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::CStyle(enumeration) => enumeration.uses_result_codec(),
            Self::Data(enumeration) => enumeration.uses_result_codec(),
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::CStyle(enumeration) => enumeration.uses_builtin_codec(kind),
            Self::Data(enumeration) => enumeration.uses_builtin_codec(kind),
        }
    }

    fn uses_direct_record_vector(&self) -> bool {
        match self {
            Self::CStyle(enumeration) => enumeration.uses_direct_record_vector(),
            Self::Data(enumeration) => enumeration.uses_direct_record_vector(),
        }
    }

    fn uses_async_execution(&self) -> bool {
        match self {
            Self::CStyle(enumeration) => enumeration.uses_async_execution(),
            Self::Data(enumeration) => enumeration.uses_async_execution(),
        }
    }
}

/// A fieldless enum whose variants are integer values.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct CStyleEnumDecl<S: Surface> {
    id: EnumId,
    name: CanonicalName,
    #[serde(default, skip_serializing_if = "DeclarationRole::is_value")]
    role: DeclarationRole,
    meta: DeclMeta,
    repr: IntegerRepr,
    variants: Vec<CStyleVariantDecl>,
    initializers: Vec<InitializerDecl<S>>,
    methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
}

impl<S: Surface> CStyleEnumDecl<S> {
    pub(crate) fn new(
        id: EnumId,
        name: CanonicalName,
        meta: DeclMeta,
        repr: IntegerRepr,
        variants: Vec<CStyleVariantDecl>,
        initializers: Vec<InitializerDecl<S>>,
        methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
    ) -> Self {
        Self {
            id,
            name,
            role: DeclarationRole::Value,
            meta,
            repr,
            variants,
            initializers,
            methods,
        }
    }

    /// Returns the enum id.
    pub const fn id(&self) -> EnumId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration role.
    pub const fn role(&self) -> DeclarationRole {
        self.role
    }

    /// Returns whether this declaration is carried by an encoded error channel.
    pub const fn is_error_payload(&self) -> bool {
        self.role.is_error_payload()
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the discriminant representation.
    pub const fn repr(&self) -> IntegerRepr {
        self.repr
    }

    /// Returns the variants in source order.
    pub fn variants(&self) -> &[CStyleVariantDecl] {
        &self.variants
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        &self.methods
    }

    fn uses_result_codec(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_result_codec)
            || self.methods.iter().any(MethodDecl::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.initializers
            .iter()
            .any(|initializer| initializer.uses_builtin_codec(kind))
            || self
                .methods
                .iter()
                .any(|method| method.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_direct_record_vector)
            || self
                .methods
                .iter()
                .any(MethodDecl::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_async_execution)
            || self.methods.iter().any(MethodDecl::uses_async_execution)
    }

    fn mark_error_payload(&mut self) {
        self.role = DeclarationRole::ErrorPayload;
    }
}

/// One variant of a fieldless enum.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CStyleVariantDecl {
    name: CanonicalName,
    discriminant: IntegerValue,
    meta: ElementMeta,
}

impl CStyleVariantDecl {
    pub(crate) fn new(name: CanonicalName, discriminant: IntegerValue, meta: ElementMeta) -> Self {
        Self {
            name,
            discriminant,
            meta,
        }
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the discriminant value.
    pub const fn discriminant(&self) -> IntegerValue {
        self.discriminant
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }
}

/// An enum whose variants can carry data.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct DataEnumDecl<S: Surface> {
    id: EnumId,
    name: CanonicalName,
    #[serde(default, skip_serializing_if = "DeclarationRole::is_value")]
    role: DeclarationRole,
    meta: DeclMeta,
    variants: Vec<DataVariantDecl>,
    initializers: Vec<InitializerDecl<S>>,
    methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
    codec: CodecPlan,
}

impl<S: Surface> DataEnumDecl<S> {
    pub(crate) fn new(
        id: EnumId,
        name: CanonicalName,
        meta: DeclMeta,
        variants: Vec<DataVariantDecl>,
        initializers: Vec<InitializerDecl<S>>,
        methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
        codec: CodecPlan,
    ) -> Self {
        Self {
            id,
            name,
            role: DeclarationRole::Value,
            meta,
            variants,
            initializers,
            methods,
            codec,
        }
    }

    /// Returns the enum id.
    pub const fn id(&self) -> EnumId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration role.
    pub const fn role(&self) -> DeclarationRole {
        self.role
    }

    /// Returns whether this declaration is carried by an encoded error channel.
    pub const fn is_error_payload(&self) -> bool {
        self.role.is_error_payload()
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the variants in source order.
    pub fn variants(&self) -> &[DataVariantDecl] {
        &self.variants
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        &self.methods
    }

    /// Returns the whole-enum read plan.
    pub fn read(&self) -> &ReadPlan {
        self.codec.read()
    }

    /// Returns the whole-enum write plan.
    pub fn write(&self) -> &WritePlan {
        self.codec.write()
    }

    /// Returns the whole-enum codec.
    pub fn codec(&self) -> &CodecPlan {
        &self.codec
    }

    fn uses_result_codec(&self) -> bool {
        self.variants.iter().any(DataVariantDecl::uses_result_codec)
            || self
                .initializers
                .iter()
                .any(InitializerDecl::uses_result_codec)
            || self.methods.iter().any(MethodDecl::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.variants
            .iter()
            .any(|variant| variant.uses_builtin_codec(kind))
            || self
                .initializers
                .iter()
                .any(|initializer| initializer.uses_builtin_codec(kind))
            || self
                .methods
                .iter()
                .any(|method| method.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_direct_record_vector)
            || self
                .methods
                .iter()
                .any(MethodDecl::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.initializers
            .iter()
            .any(InitializerDecl::uses_async_execution)
            || self.methods.iter().any(MethodDecl::uses_async_execution)
    }

    fn mark_error_payload(&mut self) {
        self.role = DeclarationRole::ErrorPayload;
    }
}

/// The integer tag assigned to one data enum variant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VariantTag(u32);

impl VariantTag {
    pub(crate) fn new(tag: u32) -> Self {
        Self(tag)
    }

    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    /// Returns the tag value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One variant of a data enum.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DataVariantDecl {
    name: CanonicalName,
    tag: VariantTag,
    payload: DataVariantPayload,
    meta: ElementMeta,
}

impl DataVariantDecl {
    pub(crate) fn new(
        name: CanonicalName,
        tag: VariantTag,
        payload: DataVariantPayload,
        meta: ElementMeta,
    ) -> Self {
        Self {
            name,
            tag,
            payload,
            meta,
        }
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the tag.
    pub const fn tag(&self) -> VariantTag {
        self.tag
    }

    /// Returns the payload shape.
    pub fn payload(&self) -> &DataVariantPayload {
        &self.payload
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    fn uses_result_codec(&self) -> bool {
        self.payload.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.payload.uses_builtin_codec(kind)
    }
}

/// The data carried by one variant of a data enum.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DataVariantPayload {
    /// Variant without payload.
    Unit,
    /// Tuple-style payload fields.
    Tuple(Vec<EncodedFieldDecl>),
    /// Struct-style payload fields.
    Struct(Vec<EncodedFieldDecl>),
}

impl DataVariantPayload {
    fn uses_result_codec(&self) -> bool {
        match self {
            Self::Tuple(fields) | Self::Struct(fields) => {
                fields.iter().any(EncodedFieldDecl::uses_result_codec)
            }
            Self::Unit => false,
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Tuple(fields) | Self::Struct(fields) => {
                fields.iter().any(|field| field.uses_builtin_codec(kind))
            }
            Self::Unit => false,
        }
    }
}

/// A free function exported across the boundary.
///
/// Carries the binding name, the native symbol foreign code links
/// against, and the [`CallableDecl`] that describes how the call
/// actually crosses.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct FunctionDecl<S: Surface> {
    id: FunctionId,
    name: CanonicalName,
    meta: DeclMeta,
    symbol: NativeSymbol,
    callable: ExportedCallable<S>,
}

impl<S: Surface> FunctionDecl<S> {
    pub(crate) fn new(
        id: FunctionId,
        name: CanonicalName,
        meta: DeclMeta,
        symbol: NativeSymbol,
        callable: ExportedCallable<S>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            symbol,
            callable,
        }
    }

    /// Returns the function id.
    pub const fn id(&self) -> FunctionId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the native symbol.
    pub fn symbol(&self) -> &NativeSymbol {
        &self.symbol
    }

    /// Returns the callable.
    pub fn callable(&self) -> &ExportedCallable<S> {
        &self.callable
    }

    fn uses_result_codec(&self) -> bool {
        self.callable.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.callable.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.callable.uses_direct_record_vector()
    }

    fn uses_async_execution(&self) -> bool {
        self.callable.uses_async_execution()
    }
}

/// A Rust type exposed as a class-style object.
///
/// Foreign code holds a handle that names a Rust-owned instance.
/// Initializers construct new instances and methods cross as ordinary
/// callables that take the handle as their receiver. The handle carrier
/// is target-divergent (`U64`/`USize` on native, `U32` on wasm), and the
/// release symbol is the native function the foreign side calls when
/// its handle goes out of scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct ClassDecl<S: Surface> {
    id: ClassId,
    name: CanonicalName,
    meta: DeclMeta,
    handle: S::HandleCarrier,
    release: NativeSymbol,
    #[serde(flatten)]
    callables: ClassCallables<S>,
}

pub(crate) struct ClassDeclParts<S: Surface> {
    pub(crate) id: ClassId,
    pub(crate) name: CanonicalName,
    pub(crate) meta: DeclMeta,
    pub(crate) thread_safety: ClassThreadSafety,
    pub(crate) handle: S::HandleCarrier,
    pub(crate) release: NativeSymbol,
    pub(crate) initializers: Vec<InitializerDecl<S>>,
    pub(crate) methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
struct ClassCallables<S: Surface> {
    #[serde(default)]
    thread_safety: ClassThreadSafety,
    initializers: Vec<InitializerDecl<S>>,
    methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum InvalidClassDecl {
    MutableReceiverRequiresUnsafeSingleThreaded,
}

/// Thread-safety requirement for an exported class handle.
///
/// Exported class handles require `Send + Sync` unless the source contract
/// explicitly declares single-threaded access.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ClassThreadSafety {
    /// The exported class type must implement `Send + Sync`.
    #[default]
    RequireSendSync,
    /// The exported class type is allowed without a `Send + Sync` assertion.
    UnsafeSingleThreaded,
}

impl ClassThreadSafety {
    /// Returns whether class exports require a `Send + Sync` assertion.
    pub const fn requires_send_sync(self) -> bool {
        matches!(self, Self::RequireSendSync)
    }
}

impl<S: Surface> ClassCallables<S> {
    fn new(
        thread_safety: ClassThreadSafety,
        initializers: Vec<InitializerDecl<S>>,
        methods: Vec<ExportedMethodDecl<S, NativeSymbol>>,
    ) -> Result<Self, InvalidClassDecl> {
        let callables = Self {
            thread_safety,
            initializers,
            methods,
        };
        match callables.invalid() {
            Some(error) => Err(error),
            None => Ok(callables),
        }
    }

    fn invalid(&self) -> Option<InvalidClassDecl> {
        (self.thread_safety.requires_send_sync()
            && self
                .methods
                .iter()
                .any(|method| method.callable().receiver() == Some(Receive::ByMutRef)))
        .then_some(InvalidClassDecl::MutableReceiverRequiresUnsafeSingleThreaded)
    }

    fn validate(&self) -> Result<(), crate::BindingError> {
        match self.invalid() {
            Some(error) => Err(crate::BindingError::from(error)),
            None => Ok(()),
        }
    }
}

impl<S: Surface> ClassDecl<S> {
    pub(crate) fn new(parts: ClassDeclParts<S>) -> Result<Self, InvalidClassDecl> {
        let callables =
            ClassCallables::new(parts.thread_safety, parts.initializers, parts.methods)?;

        Ok(Self {
            id: parts.id,
            name: parts.name,
            meta: parts.meta,
            handle: parts.handle,
            release: parts.release,
            callables,
        })
    }

    /// Returns the class id.
    pub const fn id(&self) -> ClassId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the class thread-safety policy.
    pub const fn thread_safety(&self) -> ClassThreadSafety {
        self.callables.thread_safety
    }

    pub(crate) fn validate(&self) -> Result<(), crate::BindingError> {
        self.callables.validate()
    }

    /// Returns the handle carrier.
    pub fn handle(&self) -> S::HandleCarrier {
        self.handle
    }

    /// Returns the symbol that drops a handle on the Rust side.
    pub fn release(&self) -> &NativeSymbol {
        &self.release
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl<S>] {
        &self.callables.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[ExportedMethodDecl<S, NativeSymbol>] {
        &self.callables.methods
    }

    fn uses_result_codec(&self) -> bool {
        self.initializers()
            .iter()
            .any(InitializerDecl::uses_result_codec)
            || self.methods().iter().any(MethodDecl::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.initializers()
            .iter()
            .any(|initializer| initializer.uses_builtin_codec(kind))
            || self
                .methods()
                .iter()
                .any(|method| method.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.initializers()
            .iter()
            .any(InitializerDecl::uses_direct_record_vector)
            || self
                .methods()
                .iter()
                .any(MethodDecl::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.initializers()
            .iter()
            .any(InitializerDecl::uses_async_execution)
            || self.methods().iter().any(MethodDecl::uses_async_execution)
    }
}

impl From<InvalidClassDecl> for crate::BindingError {
    fn from(value: InvalidClassDecl) -> Self {
        match value {
            InvalidClassDecl::MutableReceiverRequiresUnsafeSingleThreaded => {
                Self::new(crate::BindingErrorKind::MutableClassReceiverRequiresUnsafeSingleThreaded)
            }
        }
    }
}

/// A foreign-implemented trait whose methods Rust can call.
///
/// The dispatch surface depends on the surface: native callbacks use a
/// vtable struct, wasm callbacks use individually imported functions.
/// The IR captures the appropriate shape through `S::CallbackProtocol`
/// so renderers never reconstruct dispatch names by convention.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, S::CallbackProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, S::CallbackProtocol: serde::de::DeserializeOwned"
))]
pub struct CallbackDecl<S: Surface> {
    id: CallbackId,
    name: CanonicalName,
    meta: DeclMeta,
    handle: S::HandleCarrier,
    protocol: S::CallbackProtocol,
    local_protocol: Option<CallbackLocalProtocol<S>>,
}

impl<S: Surface> CallbackDecl<S> {
    pub(crate) fn new(
        id: CallbackId,
        name: CanonicalName,
        meta: DeclMeta,
        handle: S::HandleCarrier,
        protocol: S::CallbackProtocol,
        local_protocol: Option<CallbackLocalProtocol<S>>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            handle,
            protocol,
            local_protocol,
        }
    }

    /// Returns the callback id.
    pub const fn id(&self) -> CallbackId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the handle carrier.
    pub fn handle(&self) -> S::HandleCarrier {
        self.handle
    }

    /// Returns the dispatch protocol foreign code uses.
    pub fn protocol(&self) -> &S::CallbackProtocol {
        &self.protocol
    }

    /// Returns the Rust-side protocol for callback values implemented in Rust.
    pub fn local_protocol(&self) -> Option<&CallbackLocalProtocol<S>> {
        self.local_protocol.as_ref()
    }

    fn uses_result_codec(&self) -> bool {
        self.protocol()
            .method_callables()
            .any(ImportedCallable::uses_result_codec)
            || self
                .local_protocol()
                .is_some_and(CallbackLocalProtocol::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.protocol()
            .method_callables()
            .any(|callable| callable.uses_builtin_codec(kind))
            || self
                .local_protocol()
                .is_some_and(|protocol| protocol.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.protocol()
            .method_callables()
            .any(ImportedCallable::uses_direct_record_vector)
            || self
                .local_protocol()
                .is_some_and(CallbackLocalProtocol::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.protocol()
            .method_callables()
            .any(ImportedCallable::uses_async_execution)
            || self
                .local_protocol()
                .is_some_and(CallbackLocalProtocol::uses_async_execution)
    }
}

/// Rust-side functions backing callback values implemented in Rust.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct CallbackLocalProtocol<S: Surface> {
    handle: CallbackLocalFunction,
    free: CallbackLocalFunction,
    clone: CallbackLocalFunction,
    methods: Vec<CallbackLocalMethodDecl<S>>,
}

impl<S: Surface> CallbackLocalProtocol<S> {
    pub(crate) fn new(
        handle: CallbackLocalFunction,
        free: CallbackLocalFunction,
        clone: CallbackLocalFunction,
        methods: Vec<CallbackLocalMethodDecl<S>>,
    ) -> Self {
        Self {
            handle,
            free,
            clone,
            methods,
        }
    }

    /// Returns the helper that creates a callback handle from a Rust implementation.
    pub fn handle(&self) -> &CallbackLocalFunction {
        &self.handle
    }

    /// Returns the function that releases a local callback handle.
    pub fn free(&self) -> &CallbackLocalFunction {
        &self.free
    }

    /// Returns the function that duplicates a local callback handle.
    pub fn clone_fn(&self) -> &CallbackLocalFunction {
        &self.clone
    }

    /// Returns the local method entry points.
    pub fn methods(&self) -> &[CallbackLocalMethodDecl<S>] {
        &self.methods
    }

    fn uses_result_codec(&self) -> bool {
        self.methods
            .iter()
            .any(CallbackLocalMethodDecl::uses_result_codec)
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.methods
            .iter()
            .any(|method| method.uses_builtin_codec(kind))
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.methods
            .iter()
            .any(CallbackLocalMethodDecl::uses_direct_record_vector)
    }

    fn uses_async_execution(&self) -> bool {
        self.methods
            .iter()
            .any(CallbackLocalMethodDecl::uses_async_execution)
    }
}

/// A crate-local entry point for a Rust-owned callback method.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct CallbackLocalMethodDecl<S: Surface> {
    id: MethodId,
    name: CanonicalName,
    meta: DeclMeta,
    target: CallbackLocalFunction,
    callable: ExportedCallable<S>,
}

impl<S: Surface> CallbackLocalMethodDecl<S> {
    pub(crate) fn new(
        id: MethodId,
        name: CanonicalName,
        meta: DeclMeta,
        target: CallbackLocalFunction,
        callable: ExportedCallable<S>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            target,
            callable,
        }
    }

    /// Returns the method id.
    pub const fn id(&self) -> MethodId {
        self.id
    }

    /// Returns the canonical method name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the local entry point.
    pub fn target(&self) -> &CallbackLocalFunction {
        &self.target
    }

    /// Returns the Rust-owned callback method callable.
    pub fn callable(&self) -> &ExportedCallable<S> {
        &self.callable
    }

    fn uses_result_codec(&self) -> bool {
        self.callable.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.callable.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.callable.uses_direct_record_vector()
    }

    fn uses_async_execution(&self) -> bool {
        self.callable.uses_async_execution()
    }
}

/// A crate-local callback protocol function.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CallbackLocalFunction {
    segments: Vec<NamePart>,
}

impl CallbackLocalFunction {
    pub(crate) fn new(segments: Vec<NamePart>) -> Self {
        Self { segments }
    }

    /// Returns the crate-rooted path segments after `crate`.
    ///
    /// A root trait named `Listener` yields
    /// `["__boltffi_local_demo_listener_handle"]`. A trait in `api::Listener`
    /// yields `["api", "__boltffi_local_demo_api_listener_handle"]`.
    pub fn segments(&self) -> &[NamePart] {
        &self.segments
    }
}

/// An asynchronous sequence of values produced by Rust.
///
/// Foreign code holds a handle and pulls items through the
/// [`StreamProtocol`]: subscribe to open a session, drain buffered
/// items in batches or wait for the next one, and unsubscribe when
/// finished.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned"
))]
pub struct StreamDecl<S: Surface> {
    id: StreamId,
    name: CanonicalName,
    meta: DeclMeta,
    owner: Option<ClassId>,
    mode: StreamMode,
    handle: S::HandleCarrier,
    item: StreamItemPlan<S>,
    protocol: StreamProtocol,
}

pub(crate) struct StreamDeclParts<S: Surface> {
    pub(crate) id: StreamId,
    pub(crate) name: CanonicalName,
    pub(crate) meta: DeclMeta,
    pub(crate) owner: Option<ClassId>,
    pub(crate) mode: StreamMode,
    pub(crate) handle: S::HandleCarrier,
    pub(crate) item: StreamItemPlan<S>,
    pub(crate) protocol: StreamProtocol,
}

impl<S: Surface> StreamDecl<S> {
    pub(crate) fn new(parts: StreamDeclParts<S>) -> Self {
        Self {
            id: parts.id,
            name: parts.name,
            meta: parts.meta,
            owner: parts.owner,
            mode: parts.mode,
            handle: parts.handle,
            item: parts.item,
            protocol: parts.protocol,
        }
    }

    /// Returns the stream id.
    pub const fn id(&self) -> StreamId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the owning class, when the stream is attached to one.
    pub const fn owner(&self) -> Option<ClassId> {
        self.owner
    }

    /// Returns the source stream mode.
    pub const fn mode(&self) -> StreamMode {
        self.mode
    }

    /// Returns the handle carrier.
    pub fn handle(&self) -> S::HandleCarrier {
        self.handle
    }

    /// Returns the item transport plan.
    pub fn item(&self) -> &StreamItemPlan<S> {
        &self.item
    }

    fn uses_result_codec(&self) -> bool {
        self.item.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.item.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.item.uses_direct_record_vector()
    }

    /// Returns the consumer-side protocol.
    pub fn protocol(&self) -> &StreamProtocol {
        &self.protocol
    }
}

/// Source mode requested for a stream.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StreamMode {
    /// Values are surfaced as an asynchronous sequence.
    #[default]
    Async,
    /// Values are surfaced through batched reads.
    Batch,
    /// Values are surfaced through callback delivery.
    Callback,
}

/// How a yielded stream item crosses to the foreign consumer.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum StreamItemPlan<S: Surface> {
    /// Items are copied directly into the batch output buffer.
    Direct {
        /// Item type.
        ty: DirectValueType,
        /// Size of one item in bytes.
        size: ByteSize,
    },
    /// Items are read from an encoded batch buffer.
    Encoded {
        /// Item type.
        ty: TypeRef,
        /// Foreign-side decoder for one item.
        read: ReadPlan,
        /// Buffer shape used for the encoded batch.
        shape: S::BufferShape,
    },
}

impl<S: Surface> StreamItemPlan<S> {
    /// Renders this stream item plan through the shared stream-item walker.
    pub fn render_with<'plan, R>(&'plan self, renderer: &mut R) -> R::Output
    where
        R: StreamItemPlanRender<'plan, S>,
    {
        match self {
            Self::Direct { ty, size } => renderer.direct(ty, *size),
            Self::Encoded { ty, read, shape } => renderer.encoded(ty, read, *shape),
        }
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::Encoded { shape, .. } => Some(*shape),
            Self::Direct { .. } => None,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), crate::BindingError> {
        match self.buffer_shape() {
            Some(shape) if !shape.is_valid_in_return() => Err(crate::BindingError::new(
                crate::BindingErrorKind::SliceInReturnPosition,
            )),
            _ => Ok(()),
        }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::Encoded { read, .. } => read.uses_result(),
            Self::Direct { .. } => false,
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Encoded { read, .. } => read.uses_builtin(kind),
            Self::Direct { .. } => false,
        }
    }

    fn uses_direct_record_vector(&self) -> bool {
        matches!(
            self,
            Self::Direct {
                ty: DirectValueType::Record(_),
                ..
            }
        )
    }
}

/// Target-language rendering for stream item plans.
///
/// The shared walker owns the `StreamItemPlan` variant traversal.
/// Backends implement direct and encoded item leaves without reopening
/// the stream item enum in each target.
pub trait StreamItemPlanRender<'plan, S: Surface> {
    /// Value produced by the renderer.
    type Output;

    /// Renders a directly copied stream item.
    fn direct(&mut self, ty: &'plan DirectValueType, size: ByteSize) -> Self::Output;

    /// Renders an encoded stream item.
    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        read: &'plan ReadPlan,
        shape: S::BufferShape,
    ) -> Self::Output;
}

/// The set of native symbols foreign code uses to consume a stream.
///
/// Subscribe to receive a session token, then drive the session through
/// `pop_batch`, `wait`, and `poll`; close it with `unsubscribe`. The
/// stream itself is dropped through `free`. The classifier picks every
/// symbol at classification time so foreign code never invents names.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct StreamProtocol {
    subscribe: NativeSymbol,
    pop_batch: NativeSymbol,
    wait: NativeSymbol,
    poll: NativeSymbol,
    unsubscribe: NativeSymbol,
    free: NativeSymbol,
}

impl StreamProtocol {
    pub(crate) fn new(
        subscribe: NativeSymbol,
        pop_batch: NativeSymbol,
        wait: NativeSymbol,
        poll: NativeSymbol,
        unsubscribe: NativeSymbol,
        free: NativeSymbol,
    ) -> Self {
        Self {
            subscribe,
            pop_batch,
            wait,
            poll,
            unsubscribe,
            free,
        }
    }

    /// Returns the symbol that opens a subscription.
    pub fn subscribe(&self) -> &NativeSymbol {
        &self.subscribe
    }

    /// Returns the symbol that drains a batch of buffered items.
    pub fn pop_batch(&self) -> &NativeSymbol {
        &self.pop_batch
    }

    /// Returns the symbol that blocks until at least one item is ready.
    pub fn wait(&self) -> &NativeSymbol {
        &self.wait
    }

    /// Returns the symbol that checks readiness without blocking.
    pub fn poll(&self) -> &NativeSymbol {
        &self.poll
    }

    /// Returns the symbol that closes a subscription.
    pub fn unsubscribe(&self) -> &NativeSymbol {
        &self.unsubscribe
    }

    /// Returns the symbol that drops the stream.
    pub fn free(&self) -> &NativeSymbol {
        &self.free
    }
}

/// A named constant value the contract exposes.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct ConstantDecl<S: Surface> {
    id: ConstantId,
    name: CanonicalName,
    meta: DeclMeta,
    value: ConstantValueDecl<S>,
}

impl<S: Surface> ConstantDecl<S> {
    pub(crate) fn new(
        id: ConstantId,
        name: CanonicalName,
        meta: DeclMeta,
        value: ConstantValueDecl<S>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            value,
        }
    }

    /// Returns the constant id.
    pub const fn id(&self) -> ConstantId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the value shape.
    pub fn value(&self) -> &ConstantValueDecl<S> {
        &self.value
    }

    fn uses_result_codec(&self) -> bool {
        self.value.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.value.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.value.uses_direct_record_vector()
    }
}

/// How a constant's value is delivered to foreign code.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ConstantValueDecl<S: Surface> {
    /// Emit the literal value directly in generated source.
    Inline {
        /// Type of the inline literal.
        ty: TypeRef,
        /// The literal value.
        value: DefaultValue,
        #[doc(hidden)]
        #[serde(skip)]
        _surface: PhantomData<S>,
    },
    /// Read the value through a native accessor.
    Accessor {
        /// Native symbol the accessor links against.
        symbol: NativeSymbol,
        /// Call shape of the accessor.
        callable: Box<ExportedCallable<S>>,
    },
}

impl<S: Surface> ConstantValueDecl<S> {
    /// Builds an inline constant value.
    pub fn inline(ty: TypeRef, value: DefaultValue) -> Self {
        Self::Inline {
            ty,
            value,
            _surface: PhantomData,
        }
    }

    /// Builds an accessor constant value.
    ///
    /// `symbol` is the native symbol foreign code links against to read
    /// the value at runtime. `callable` is the zero-argument exported
    /// getter that returns the constant's declared type. Used for values
    /// that have no portable inline literal (byte strings, arrays,
    /// tuples, and unevaluated expressions).
    pub fn accessor(symbol: NativeSymbol, callable: Box<ExportedCallable<S>>) -> Self {
        Self::Accessor { symbol, callable }
    }

    fn uses_result_codec(&self) -> bool {
        match self {
            Self::Accessor { callable, .. } => callable.uses_result_codec(),
            Self::Inline { .. } => false,
        }
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Accessor { callable, .. } => callable.uses_builtin_codec(kind),
            Self::Inline { ty, .. } => ty.uses_builtin(kind),
        }
    }

    fn uses_direct_record_vector(&self) -> bool {
        match self {
            Self::Accessor { callable, .. } => callable.uses_direct_record_vector(),
            Self::Inline { .. } => false,
        }
    }
}

/// A user-defined Rust type carried through an existing binding shape.
///
/// The declaration records both the representation visible at the FFI boundary
/// and the Rust conversion expressions generated wrappers must call at the
/// boundary.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomTypeDecl {
    id: CustomTypeId,
    name: CanonicalName,
    meta: DeclMeta,
    representation: TypeRef,
    converters: CustomTypeConverters,
}

impl CustomTypeDecl {
    pub(crate) fn new(
        id: CustomTypeId,
        name: CanonicalName,
        meta: DeclMeta,
        representation: TypeRef,
        converters: CustomTypeConverters,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            representation,
            converters,
        }
    }

    /// Returns the custom type id.
    pub const fn id(&self) -> CustomTypeId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the representation visible at the FFI boundary.
    pub fn representation(&self) -> &TypeRef {
        &self.representation
    }

    /// Returns the Rust converters used by generated wrappers.
    pub fn converters(&self) -> &CustomTypeConverters {
        &self.converters
    }
}

/// A method on a record, enum, class, or callback trait.
///
/// Owned by its parent declaration. Generic over the surface `S`, the
/// callable scope `K` (which side implements the body), and the
/// dispatch-target type `T`. Use the [`ExportedMethodDecl`] alias when
/// `K = RustBody` and [`ImportedMethodDecl`] when `K = ForeignBody`. `T`
/// is [`NativeSymbol`] for methods on records, enums, and classes;
/// callback trait methods use whichever target name the surface's
/// callback protocol picks ([`VTableSlot`] on native, [`ImportSymbol`]
/// on wasm32).
///
/// [`VTableSlot`]: crate::VTableSlot
/// [`ImportSymbol`]: crate::ImportSymbol
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "T: Serialize, S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::IncomingClosureRegistration: Serialize, S::OutgoingClosureRegistration: Serialize, S::AsyncProtocol: Serialize, K::ParamDirection: crate::ParamDirection<S>, K::ReturnDirection: crate::Direction, <K::ParamDirection as crate::ParamDirection<S>>::Payload: Serialize, <K::ReturnDirection as crate::Direction>::Codec: Serialize, <K::ReturnDirection as crate::Direction>::Receive: Serialize",
    deserialize = "T: serde::de::DeserializeOwned, S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::IncomingClosureRegistration: serde::de::DeserializeOwned, S::OutgoingClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, K::ParamDirection: crate::ParamDirection<S>, K::ReturnDirection: crate::Direction, <K::ParamDirection as crate::ParamDirection<S>>::Payload: serde::de::DeserializeOwned, <K::ReturnDirection as crate::Direction>::Codec: serde::de::DeserializeOwned, <K::ReturnDirection as crate::Direction>::Receive: serde::de::DeserializeOwned"
))]
pub struct MethodDecl<S: Surface, K: CallableScope, T>
where
    K::ParamDirection: crate::ParamDirection<S>,
{
    id: MethodId,
    name: CanonicalName,
    meta: DeclMeta,
    target: T,
    callable: crate::CallableDecl<S, K>,
}

impl<S: Surface, K: CallableScope, T> MethodDecl<S, K, T>
where
    K::ParamDirection: crate::ParamDirection<S>,
{
    pub(crate) fn new(
        id: MethodId,
        name: CanonicalName,
        meta: DeclMeta,
        target: T,
        callable: crate::CallableDecl<S, K>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            target,
            callable,
        }
    }

    /// Returns the method id.
    pub const fn id(&self) -> MethodId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the call target.
    pub fn target(&self) -> &T {
        &self.target
    }

    /// Returns the callable.
    pub fn callable(&self) -> &crate::CallableDecl<S, K> {
        &self.callable
    }

    fn uses_result_codec(&self) -> bool {
        self.callable.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.callable.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.callable.uses_direct_record_vector()
    }

    fn uses_async_execution(&self) -> bool {
        self.callable.uses_async_execution()
    }
}

/// A method whose body is implemented in Rust. The contained
/// callable flows params [`IntoRust`](crate::IntoRust) and returns
/// [`OutOfRust`](crate::OutOfRust).
pub type ExportedMethodDecl<S, T> = MethodDecl<S, RustBody, T>;

/// A method whose body is implemented in foreign code. The contained
/// callable flows params [`OutOfRust`](crate::OutOfRust) (Rust pushes
/// args) and returns [`IntoRust`](crate::IntoRust) (foreign produces
/// the return).
pub type ImportedMethodDecl<S, T> = MethodDecl<S, crate::ForeignBody, T>;

/// A callable selected to be exposed as a target language constructor.
///
/// Rust does not have constructors; it has associated functions that
/// happen to return `Self`. The classifier picks a subset of those and
/// promotes them to initializers so target languages can spell them as
/// `Point.init(x:y:)`, `Point(x = ..., y = ...)`, or whatever the host
/// idiom is.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct InitializerDecl<S: Surface> {
    id: InitializerId,
    name: CanonicalName,
    meta: DeclMeta,
    symbol: NativeSymbol,
    callable: ExportedCallable<S>,
    returns: ReturnTypeRef,
}

impl<S: Surface> InitializerDecl<S> {
    pub(crate) fn new(
        id: InitializerId,
        name: CanonicalName,
        meta: DeclMeta,
        symbol: NativeSymbol,
        callable: ExportedCallable<S>,
        returns: ReturnTypeRef,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            symbol,
            callable,
            returns,
        }
    }

    /// Returns the initializer id.
    pub const fn id(&self) -> InitializerId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the native symbol.
    pub fn symbol(&self) -> &NativeSymbol {
        &self.symbol
    }

    /// Returns the callable.
    pub fn callable(&self) -> &ExportedCallable<S> {
        &self.callable
    }

    /// Returns the constructed type.
    pub fn returns(&self) -> &ReturnTypeRef {
        &self.returns
    }

    fn uses_result_codec(&self) -> bool {
        self.callable.uses_result_codec()
    }

    fn uses_builtin_codec(&self, kind: BuiltinType) -> bool {
        self.callable.uses_builtin_codec(kind)
    }

    fn uses_direct_record_vector(&self) -> bool {
        self.callable.uses_direct_record_vector()
    }

    fn uses_async_execution(&self) -> bool {
        self.callable.uses_async_execution()
    }
}
