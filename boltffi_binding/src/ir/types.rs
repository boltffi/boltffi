use std::collections::BTreeSet;

use boltffi_ast::BuiltinType;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{
    ByteAlignment, ByteSize, CallbackId, ClassId, CustomTypeId, DeclarationId, EnumId, Primitive,
    RecordId, StreamId,
};

/// The value a binding declaration accepts or returns.
///
/// Higher-level than [`Primitive`]: covers the heap-managed primitives
/// the contract treats specially (`String`, `Bytes`), references to
/// user-declared types (`Record`, `Enum`, `Class`, `Callback`, `Custom`),
/// and the container shapes (`Optional`, `Sequence`, `Tuple`, `Result`, `Map`).
///
/// Source spelling is gone by the time a value reaches `TypeRef`. A Rust
/// `Option<Vec<UserProfile>>` is represented as
/// `Optional(Sequence(Record(id_of_user_profile)))`; whether it renders as
/// `[UserProfile]?` in Swift or `list[UserProfile] | None` in Python is a
/// later decision.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TypeRef {
    /// Primitive scalar value.
    Primitive(Primitive),
    /// UTF-8 string value.
    String,
    /// UTF-8 string value with a backend-provided static intern table.
    InternedString {
        /// Static values addressable by wire id before falling back to dynamic bytes.
        static_values: Vec<String>,
    },
    /// Byte buffer value.
    Bytes,
    /// Record reference.
    Record(RecordId),
    /// Enum reference.
    Enum(EnumId),
    /// Class reference.
    Class(ClassId),
    /// Callback reference.
    Callback(CallbackId),
    /// Custom type reference.
    Custom(CustomTypeId),
    /// Builtin value reference.
    Builtin(BuiltinType),
    /// Optional value.
    Optional(Box<TypeRef>),
    /// Sequence value.
    Sequence(Box<TypeRef>),
    /// Tuple value.
    Tuple(Vec<TypeRef>),
    /// Fallible value.
    Result {
        /// Success type.
        ok: Box<TypeRef>,
        /// Error type.
        err: Box<TypeRef>,
    },
    /// Map value.
    Map {
        /// Key type.
        key: Box<TypeRef>,
        /// Value type.
        value: Box<TypeRef>,
    },
}

impl TypeRef {
    /// Returns whether this type tree contains the given builtin value.
    pub fn uses_builtin(&self, kind: BuiltinType) -> bool {
        match self {
            Self::Builtin(builtin) => *builtin == kind,
            Self::Optional(inner) | Self::Sequence(inner) => inner.uses_builtin(kind),
            Self::Tuple(elements) => elements.iter().any(|element| element.uses_builtin(kind)),
            Self::Result { ok, err } => ok.uses_builtin(kind) || err.uses_builtin(kind),
            Self::Map { key, value } => key.uses_builtin(kind) || value.uses_builtin(kind),
            _ => false,
        }
    }

    /// Appends every family-tagged declaration referenced by this type tree.
    pub fn append_referenced_declarations(&self, references: &mut BTreeSet<DeclarationId>) {
        match self {
            Self::Record(id) => {
                references.insert(DeclarationId::Record(*id));
            }
            Self::Enum(id) => {
                references.insert(DeclarationId::Enum(*id));
            }
            Self::Class(id) => {
                references.insert(DeclarationId::Class(*id));
            }
            Self::Callback(id) => {
                references.insert(DeclarationId::Callback(*id));
            }
            Self::Custom(id) => {
                references.insert(DeclarationId::CustomType(*id));
            }
            Self::Optional(inner) | Self::Sequence(inner) => {
                inner.append_referenced_declarations(references);
            }
            Self::Tuple(elements) => elements
                .iter()
                .for_each(|element| element.append_referenced_declarations(references)),
            Self::Result { ok, err } => {
                ok.append_referenced_declarations(references);
                err.append_referenced_declarations(references);
            }
            Self::Map { key, value } => {
                key.append_referenced_declarations(references);
                value.append_referenced_declarations(references);
            }
            Self::Primitive(_)
            | Self::String
            | Self::InternedString { .. }
            | Self::Bytes
            | Self::Builtin(_) => {}
        }
    }

    /// Returns whether this type tree references a declaration.
    pub fn references_declaration(&self, declaration: DeclarationId) -> bool {
        let mut references = BTreeSet::new();
        self.append_referenced_declarations(&mut references);
        references.contains(&declaration)
    }

    /// Returns whether this type tree contains an [`InternedString`](TypeRef::InternedString).
    ///
    /// Used by the capability gate to detect bindings that require the
    /// `InternedString` host capability.
    pub fn contains_interned_string(&self) -> bool {
        match self {
            Self::InternedString { .. } => true,
            Self::Optional(inner) | Self::Sequence(inner) => inner.contains_interned_string(),
            Self::Tuple(elements) => elements.iter().any(TypeRef::contains_interned_string),
            Self::Result { ok, err } => {
                ok.contains_interned_string() || err.contains_interned_string()
            }
            Self::Map { key, value } => {
                key.contains_interned_string() || value.contains_interned_string()
            }
            _ => false,
        }
    }
}

/// A primitive that can cross through direct-vector transport.
///
/// `Vec<u8>` is reserved for byte-buffer transport, so `u8` is not a
/// valid direct-vector primitive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectVectorPrimitive(Primitive);

impl DirectVectorPrimitive {
    /// Returns a direct-vector primitive when the scalar is not `u8`.
    pub const fn new(primitive: Primitive) -> Option<Self> {
        match primitive {
            Primitive::U8 => None,
            _ => Some(Self(primitive)),
        }
    }

    /// Returns the scalar primitive.
    pub const fn primitive(self) -> Primitive {
        self.0
    }
}

impl Serialize for DirectVectorPrimitive {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DirectVectorPrimitive {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let primitive = Primitive::deserialize(deserializer)?;
        Self::new(primitive).ok_or_else(|| de::Error::custom("u8 uses byte-buffer transport"))
    }
}

/// Primitive type admitted by direct record layout.
///
/// Direct records are copied as target-independent bytes, so fields with
/// pointer-width-dependent layout (`isize` and `usize`) are not valid direct
/// fields.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DirectFieldType(Primitive);

impl DirectFieldType {
    /// Returns a direct field type when the primitive has fixed ABI width.
    pub const fn new(primitive: Primitive) -> Option<Self> {
        match primitive {
            Primitive::ISize | Primitive::USize => None,
            _ => Some(Self(primitive)),
        }
    }

    /// Returns the scalar primitive.
    pub const fn primitive(self) -> Primitive {
        self.0
    }

    /// Returns the field width in bytes.
    pub const fn byte_size(self) -> ByteSize {
        ByteSize::new(match self.0 {
            Primitive::Bool | Primitive::I8 | Primitive::U8 => 1,
            Primitive::I16 | Primitive::U16 => 2,
            Primitive::I32 | Primitive::U32 | Primitive::F32 => 4,
            Primitive::I64 | Primitive::U64 | Primitive::F64 => 8,
            Primitive::ISize | Primitive::USize => unreachable!(),
        })
    }

    /// Returns the field ABI alignment.
    pub const fn byte_alignment(self) -> ByteAlignment {
        match ByteAlignment::new(match self.0 {
            Primitive::Bool | Primitive::I8 | Primitive::U8 => 1,
            Primitive::I16 | Primitive::U16 => 2,
            Primitive::I32 | Primitive::U32 | Primitive::F32 => 4,
            Primitive::I64 | Primitive::U64 | Primitive::F64 => 8,
            Primitive::ISize | Primitive::USize => unreachable!(),
        }) {
            Ok(alignment) => alignment,
            Err(_) => unreachable!(),
        }
    }
}

impl Serialize for DirectFieldType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DirectFieldType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let primitive = Primitive::deserialize(deserializer)?;
        Self::new(primitive)
            .ok_or_else(|| de::Error::custom("pointer-width primitives require encoded transport"))
    }
}

/// Element type admitted by direct-vector transport.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DirectVectorElementType {
    /// Primitive element copied directly.
    Primitive(DirectVectorPrimitive),
    /// Direct record element copied through its passable representation.
    Record(RecordId),
}

/// Value type admitted by direct ABI transport.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DirectValueType {
    /// Primitive scalar copied directly.
    Primitive(Primitive),
    /// Direct record copied through its passable representation.
    Record(RecordId),
    /// C-style enum copied through its integer representation.
    Enum(EnumId),
}

impl DirectValueType {
    /// Returns a direct primitive value type.
    pub const fn primitive(primitive: Primitive) -> Self {
        Self::Primitive(primitive)
    }

    /// Returns a direct record value type.
    pub const fn record(record: RecordId) -> Self {
        Self::Record(record)
    }

    /// Returns a direct enum value type.
    pub const fn enumeration(enumeration: EnumId) -> Self {
        Self::Enum(enumeration)
    }
}

impl DirectVectorElementType {
    /// Returns a direct-vector element for a non-`u8` primitive.
    pub const fn primitive(primitive: Primitive) -> Option<Self> {
        match DirectVectorPrimitive::new(primitive) {
            Some(primitive) => Some(Self::Primitive(primitive)),
            None => None,
        }
    }

    /// Returns a direct-vector element for a direct record.
    pub const fn record(record: RecordId) -> Self {
        Self::Record(record)
    }
}

impl TypeRef {
    /// Returns whether this type is the `u8` primitive.
    pub const fn is_u8_primitive(&self) -> bool {
        matches!(self, Self::Primitive(Primitive::U8))
    }

    /// Returns the primitive scalar when this type is primitive.
    pub const fn primitive(&self) -> Option<Primitive> {
        match self {
            Self::Primitive(primitive) => Some(*primitive),
            _ => None,
        }
    }

    /// Renders this type through the shared type walker.
    pub fn render_with<R>(&self, renderer: &mut R) -> R::Output
    where
        R: TypeRefRender,
    {
        TypeRefWalker::render(self, renderer)
    }
}

/// Target-language rendering for [`TypeRef`] leaves and containers.
pub trait TypeRefRender {
    /// Target value produced by the renderer.
    type Output;

    /// Renders a primitive scalar.
    fn primitive(&mut self, primitive: Primitive) -> Self::Output;

    /// Renders a UTF-8 string.
    fn string(&mut self) -> Self::Output;

    /// Renders a tagged interned-string value.
    fn interned_string(&mut self, static_values: &[String]) -> Self::Output;

    /// Renders a byte buffer.
    fn bytes(&mut self) -> Self::Output;

    /// Renders a record reference.
    fn record(&mut self, id: RecordId) -> Self::Output;

    /// Renders an enum reference.
    fn enumeration(&mut self, id: EnumId) -> Self::Output;

    /// Renders a class reference.
    fn class(&mut self, id: ClassId) -> Self::Output;

    /// Renders a callback reference.
    fn callback(&mut self, id: CallbackId) -> Self::Output;

    /// Renders a custom type reference.
    fn custom(&mut self, id: CustomTypeId) -> Self::Output;

    /// Renders a builtin value reference.
    fn builtin(&mut self, kind: BuiltinType) -> Self::Output;

    /// Renders an optional value.
    fn optional(&mut self, inner: Self::Output) -> Self::Output;

    /// Renders a sequence value.
    fn sequence(&mut self, element: Self::Output) -> Self::Output;

    /// Renders a tuple value.
    fn tuple(&mut self, elements: Vec<Self::Output>) -> Self::Output;

    /// Renders a result value.
    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output;

    /// Renders a map value.
    fn map(&mut self, key: Self::Output, value: Self::Output) -> Self::Output;
}

struct TypeRefWalker;

impl TypeRefWalker {
    fn render<R>(ty: &TypeRef, renderer: &mut R) -> R::Output
    where
        R: TypeRefRender,
    {
        match ty {
            TypeRef::Primitive(primitive) => renderer.primitive(*primitive),
            TypeRef::String => renderer.string(),
            TypeRef::InternedString { static_values } => renderer.interned_string(static_values),
            TypeRef::Bytes => renderer.bytes(),
            TypeRef::Record(id) => renderer.record(*id),
            TypeRef::Enum(id) => renderer.enumeration(*id),
            TypeRef::Class(id) => renderer.class(*id),
            TypeRef::Callback(id) => renderer.callback(*id),
            TypeRef::Custom(id) => renderer.custom(*id),
            TypeRef::Builtin(kind) => renderer.builtin(*kind),
            TypeRef::Optional(inner) => {
                let inner = Self::render(inner, renderer);
                renderer.optional(inner)
            }
            TypeRef::Sequence(element) => {
                let element = Self::render(element, renderer);
                renderer.sequence(element)
            }
            TypeRef::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| Self::render(element, renderer))
                    .collect();
                renderer.tuple(elements)
            }
            TypeRef::Result { ok, err } => {
                let ok = Self::render(ok, renderer);
                let err = Self::render(err, renderer);
                renderer.result(ok, err)
            }
            TypeRef::Map { key, value } => {
                let key = Self::render(key, renderer);
                let value = Self::render(value, renderer);
                renderer.map(key, value)
            }
        }
    }
}

/// The result type of a callable, including the absence of a result.
///
/// `()` is meaningful in a return position and meaningless as a field or
/// parameter type, so a separate wrapper keeps the latter from accepting a
/// "void" value.
///
/// # Example
///
/// `ReturnTypeRef::Void` for `fn save() -> ()`,
/// `ReturnTypeRef::Value(TypeRef::Primitive(Primitive::I32))` for
/// `fn count() -> i32`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReturnTypeRef {
    /// The callable returns no value.
    Void,
    /// The callable returns one value.
    Value(TypeRef),
}

/// What an opaque handle stands in for.
///
/// Handles cross the boundary as integer tokens; the variants name the
/// kinds of declarations a token can refer to. Excludes value-shaped
/// types like primitives, records, and enums, which never cross as
/// opaque tokens. Narrower than [`TypeRef`] so the type system rejects
/// "handle to `i32`" or "handle to `Point`" at the construction site.
///
/// # Example
///
/// A `Class` handle into a Rust-owned `Engine` instance is represented
/// as `HandleTarget::Class(engine_id)`. A foreign-implemented callback
/// trait is `HandleTarget::Callback(listener_id)`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HandleTarget {
    /// Class instance owned by Rust.
    Class(ClassId),
    /// Callback object implemented on the foreign side.
    Callback(CallbackId),
    /// Stream of values produced by Rust.
    Stream(StreamId),
}

/// Whether a handle-typed slot is always populated or may be absent.
///
/// Nullability is a per-site decision on the dispatch plan, not a
/// property of the target type. The same callback trait can be required
/// on one method and nullable on another. The wire shape is identical;
/// the carrier is the same width and a zero/null sentinel encodes the
/// absent state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HandlePresence {
    /// Caller must supply a live handle.
    Required,
    /// Caller may omit the handle; a zero/null sentinel encodes absence.
    Nullable,
}
