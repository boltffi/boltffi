use std::{error::Error, fmt};

use crate::BindingError;

/// An error returned while lowering source into a binding contract.
///
/// Lowering fails before a [`Bindings`](crate::Bindings) value exists.
/// Failures here describe source shapes the lowering pass cannot represent
/// yet, unresolved declaration references, or invalid binding values
/// rejected during final validation.
#[derive(Debug)]
pub struct LowerError {
    kind: LowerErrorKind,
}

impl LowerError {
    pub(crate) fn new(kind: LowerErrorKind) -> Self {
        Self { kind }
    }

    pub(crate) fn unsupported_declaration(family: DeclarationFamily) -> Self {
        Self::new(LowerErrorKind::UnsupportedDeclaration(family))
    }

    pub(crate) fn unsupported_type(unsupported: UnsupportedType) -> Self {
        Self::new(LowerErrorKind::UnsupportedType(unsupported))
    }

    pub(crate) fn duplicate_source_id(family: DeclarationFamily, id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::DuplicateSourceId {
            family,
            id: id.to_string(),
        })
    }

    pub(crate) fn unknown_record(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownRecord(id.to_string()))
    }

    pub(crate) fn unknown_enum(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownEnum(id.to_string()))
    }

    pub(crate) fn unknown_class(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownClass(id.to_string()))
    }

    pub(crate) fn unknown_callback(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownCallback(id.to_string()))
    }

    pub(crate) fn unknown_custom(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownCustom(id.to_string()))
    }

    pub(crate) fn unknown_constant(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownConstant(id.to_string()))
    }

    pub(crate) fn invalid_constant_value(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::InvalidConstantValue(id.to_string()))
    }

    pub(crate) fn unknown_stream(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownStream(id.to_string()))
    }

    pub(crate) fn unknown_function(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownFunction(id.to_string()))
    }

    pub(crate) fn invalid_alignment(bytes: u64) -> Self {
        Self::new(LowerErrorKind::InvalidAlignment(bytes))
    }

    pub(crate) fn discriminant_overflow() -> Self {
        Self::new(LowerErrorKind::DiscriminantOverflow)
    }

    pub(crate) fn variant_tag_overflow() -> Self {
        Self::new(LowerErrorKind::VariantTagOverflow)
    }

    pub(crate) fn field_position_overflow() -> Self {
        Self::new(LowerErrorKind::FieldPositionOverflow)
    }

    /// Returns the reason lowering failed.
    pub fn kind(&self) -> &LowerErrorKind {
        &self.kind
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            LowerErrorKind::UnsupportedDeclaration(family) => {
                write!(formatter, "{} lowering is not implemented", family)
            }
            LowerErrorKind::UnsupportedType(unsupported) => {
                write!(
                    formatter,
                    "{} cannot be represented in binding IR yet",
                    unsupported
                )
            }
            LowerErrorKind::InvalidFunctionForm => {
                formatter.write_str("function declaration has non-function callable form")
            }
            LowerErrorKind::DuplicateSourceId { family, id } => {
                write!(formatter, "duplicate {} source id `{id}`", family)
            }
            LowerErrorKind::UnknownRecord(record) => {
                write!(formatter, "unknown record id `{record}`")
            }
            LowerErrorKind::UnknownEnum(enumeration) => {
                write!(formatter, "unknown enum id `{enumeration}`")
            }
            LowerErrorKind::UnknownClass(class) => {
                write!(formatter, "unknown class id `{class}`")
            }
            LowerErrorKind::UnknownCallback(callback) => {
                write!(formatter, "unknown callback id `{callback}`")
            }
            LowerErrorKind::UnknownCustom(custom) => {
                write!(formatter, "unknown custom type id `{custom}`")
            }
            LowerErrorKind::UnknownConstant(constant) => {
                write!(formatter, "unknown constant id `{constant}`")
            }
            LowerErrorKind::InvalidConstantValue(constant) => {
                write!(
                    formatter,
                    "constant `{constant}` value does not match its declared type"
                )
            }
            LowerErrorKind::UnknownStream(stream) => {
                write!(formatter, "unknown stream id `{stream}`")
            }
            LowerErrorKind::UnknownFunction(function) => {
                write!(formatter, "unknown function id `{function}`")
            }
            LowerErrorKind::InvalidAlignment(alignment) => {
                write!(formatter, "invalid record alignment {alignment}")
            }
            LowerErrorKind::DiscriminantOverflow => {
                formatter.write_str("enum discriminant overflow")
            }
            LowerErrorKind::VariantTagOverflow => formatter.write_str("enum variant tag overflow"),
            LowerErrorKind::FieldPositionOverflow => formatter.write_str("field position overflow"),
            LowerErrorKind::InvalidBindings(error) => error.fmt(formatter),
        }
    }
}

impl Error for LowerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            LowerErrorKind::InvalidBindings(error) => Some(error),
            _ => None,
        }
    }
}

/// The reason source lowering failed.
///
/// Variants are precise enough for diagnostics to point at the unsupported
/// declaration family, unresolved reference, or invalid value that stopped
/// construction.
#[derive(Debug)]
#[non_exhaustive]
pub enum LowerErrorKind {
    /// A top-level declaration family is not lowered by this slice yet.
    UnsupportedDeclaration(DeclarationFamily),
    /// A source type has no binding-IR representation yet.
    UnsupportedType(UnsupportedType),
    /// A function declaration carried a non-function source placement.
    InvalidFunctionForm,
    /// Two declarations in the same family share one source id.
    DuplicateSourceId {
        /// Declaration family where the duplicate was found.
        family: DeclarationFamily,
        /// Duplicated source id.
        id: String,
    },
    /// A record reference could not be resolved inside the source contract.
    UnknownRecord(String),
    /// An enum reference could not be resolved inside the source contract.
    UnknownEnum(String),
    /// A class reference could not be resolved inside the source contract.
    UnknownClass(String),
    /// A callback reference could not be resolved inside the source contract.
    UnknownCallback(String),
    /// A custom type reference could not be resolved inside the source contract.
    UnknownCustom(String),
    /// A constant reference could not be resolved inside the source contract.
    UnknownConstant(String),
    /// A constant literal cannot inhabit its declared type.
    InvalidConstantValue(String),
    /// A stream reference could not be resolved inside the source contract.
    UnknownStream(String),
    /// A free function reference could not be resolved inside the source contract.
    UnknownFunction(String),
    /// A computed record alignment was not a valid ABI alignment.
    InvalidAlignment(u64),
    /// An enum discriminant sequence overflowed `i128`.
    DiscriminantOverflow,
    /// A data enum variant index could not fit in a variant tag.
    VariantTagOverflow,
    /// A tuple field index could not fit in a field position.
    FieldPositionOverflow,
    /// The lowered contract failed binding validation.
    InvalidBindings(BindingError),
}

/// A declaration family known to the lowering pass.
///
/// Used in diagnostics when an entire family is unsupported or contains
/// duplicate source IDs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DeclarationFamily {
    /// Record declarations.
    Records,
    /// Enum declarations.
    Enums,
    /// Free function declarations.
    Functions,
    /// Class-style object declarations.
    Classes,
    /// Trait declarations.
    Traits,
    /// Stream declarations.
    Streams,
    /// Constant declarations.
    Constants,
    /// Custom type declarations.
    CustomTypes,
    /// Methods attached to records.
    RecordMethods,
    /// Methods attached to enums.
    EnumMethods,
}

impl fmt::Display for DeclarationFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Records => "records",
            Self::Enums => "enums",
            Self::Functions => "functions",
            Self::Classes => "classes",
            Self::Traits => "traits",
            Self::Streams => "streams",
            Self::Constants => "constants",
            Self::CustomTypes => "custom types",
            Self::RecordMethods => "record methods",
            Self::EnumMethods => "enum methods",
        })
    }
}

/// A source type shape the lowering pass cannot represent yet.
///
/// These are gaps in the current binding-IR slice, not generic parse errors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum UnsupportedType {
    /// A direct record field was not a fixed-width primitive.
    RecordField,
    /// An enum representation did not resolve to an integer carrier.
    EnumRepr,
    /// `Self` appeared where the lowering pass did not have an owning declaration.
    SelfType,
    /// A generic type parameter appeared in exported source.
    TypeParameter,
    /// A default value cannot be emitted as binding metadata yet.
    DefaultValue,
    /// `()` appeared in a position that requires a value-shaped type (a field
    /// or parameter). Unit is only meaningful as a callable result; carrying
    /// it as a value would force every consumer to special-case empty data.
    UnitInValuePosition,
    /// `Self` appeared in a callback trait signature where the trait object
    /// has no concrete implementer type to substitute.
    SelfInCallbackTrait,
    /// A callback method snake-cases to a name already taken by the vtable
    /// lifecycle slot (`free` or `clone`) or by another callback method.
    CallbackMethodSlotCollision,
    /// A callback method declared a receiver the callback handle protocol
    /// cannot dispatch.
    InvalidCallbackReceiver,
    /// A callback handle parameter was borrowed instead of passed by value.
    BorrowedCallbackParameter,
    /// An owned class receiver has no handle-transfer protocol yet.
    OwnedClassReceiver,
    /// A mutable class receiver needs the class to be exported as unsafe
    /// single-threaded because `Send + Sync` does not make concurrent mutable
    /// borrows sound.
    MutableClassReceiverRequiresUnsafeSingleThreaded,
    /// An inline closure appeared inside a value-shaped position
    /// (record field, vec element, tuple element, map key or value,
    /// `Option` inner, ...). Closures cross at callable parameter
    /// slots and at the return slot of a callable that returns one,
    /// not as elements embedded in encoded values.
    ClosureInValuePosition,
    /// A Rust-owned pointer container appeared where only callback-handle
    /// carrier spellings are supported.
    OpaqueRustContainer,
    /// A stream item type was not wire-encodable.
    StreamItem,
    /// A custom type converter cannot be represented as a binding converter.
    CustomConverter,
    /// An interned string cannot currently serve as a custom type representation
    /// because representation conversion does not retain the pool's Rust type.
    InternedStringCustomRepresentation,
}

impl fmt::Display for UnsupportedType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RecordField => "record field",
            Self::EnumRepr => "enum repr",
            Self::SelfType => "Self",
            Self::TypeParameter => "type parameter",
            Self::DefaultValue => "default value",
            Self::UnitInValuePosition => "unit type `()` in a value position",
            Self::SelfInCallbackTrait => "Self in callback trait method",
            Self::CallbackMethodSlotCollision => "callback method name collides with vtable slot",
            Self::InvalidCallbackReceiver => "callback method receiver",
            Self::BorrowedCallbackParameter => "borrowed callback parameter",
            Self::OwnedClassReceiver => "owned class receiver",
            Self::MutableClassReceiverRequiresUnsafeSingleThreaded => {
                "mutable class receiver without unsafe single-threaded export"
            }
            Self::ClosureInValuePosition => "closure in a value-shaped position",
            Self::OpaqueRustContainer => "opaque Rust pointer container",
            Self::StreamItem => "stream item type",
            Self::CustomConverter => "custom type converter",
            Self::InternedStringCustomRepresentation => "InternedString custom type representation",
        })
    }
}
