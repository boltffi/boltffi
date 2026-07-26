use std::{error::Error, fmt};

use crate::{ContractVersion, DeclarationId, DeclarationShape, SymbolId};

/// A reason a binding contract could not be exposed.
///
/// Returned only at construction boundaries: deserialization, symbol-table
/// building, validation. Once a [`Bindings`](crate::Bindings) value is
/// held, the failures listed in [`BindingErrorKind`] cannot occur.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BindingError {
    kind: BindingErrorKind,
}

impl BindingError {
    /// Builds an error from a specific failure.
    pub fn new(kind: BindingErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the failure that produced this error.
    pub fn kind(&self) -> &BindingErrorKind {
        &self.kind
    }
}

impl fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            BindingErrorKind::UnsupportedVersion { actual, current } => write!(
                formatter,
                "binding contract version {}.{} cannot be read by {}.{}",
                actual.major(),
                actual.minor(),
                current.major(),
                current.minor()
            ),
            BindingErrorKind::DuplicateDeclarationId(id) => {
                write!(formatter, "duplicate declaration id {id:?}")
            }
            BindingErrorKind::UnknownDeclarationId(id) => {
                write!(formatter, "unknown declaration id {id:?}")
            }
            BindingErrorKind::MissingDeclarationReference { owner, referenced } => write!(
                formatter,
                "declaration {owner:?} references missing declaration {referenced:?}"
            ),
            BindingErrorKind::InvalidDeclarationReference {
                owner,
                referenced,
                expected,
            } => write!(
                formatter,
                "declaration {owner:?} references {referenced:?} as {expected}, but the declaration has a different shape"
            ),
            BindingErrorKind::DuplicateSymbolId(id) => {
                write!(formatter, "duplicate native symbol id {id:?}")
            }
            BindingErrorKind::DuplicateSymbolName(name) => {
                write!(formatter, "duplicate native symbol name `{name}`")
            }
            BindingErrorKind::InvalidSymbolName(name) => {
                write!(formatter, "invalid native symbol name `{name}`")
            }
            BindingErrorKind::InvalidVTableSlot(name) => {
                write!(formatter, "invalid vtable slot name `{name}`")
            }
            BindingErrorKind::InvalidImportModule(name) => {
                write!(formatter, "invalid wasm import module `{name}`")
            }
            BindingErrorKind::UnregisteredSymbol(name) => {
                write!(
                    formatter,
                    "native symbol `{name}` is absent from the symbol table"
                )
            }
            BindingErrorKind::UnreferencedSymbol(name) => {
                write!(
                    formatter,
                    "native symbol `{name}` is not referenced by a declaration"
                )
            }
            BindingErrorKind::ReturnSlotConflict => {
                formatter.write_str("callable return and error both claim the native return slot")
            }
            BindingErrorKind::PackedInParamPosition => formatter
                .write_str("BufferShape::Packed cannot appear on a parameter encoded crossing"),
            BindingErrorKind::SliceInReturnPosition => formatter.write_str(
                "BufferShape::Slice cannot appear on a return or error encoded crossing",
            ),
            BindingErrorKind::MutableClassReceiverRequiresUnsafeSingleThreaded => formatter
                .write_str("mutable class receivers require UnsafeSingleThreaded class export"),
        }
    }
}

impl Error for BindingError {}

/// The specific failure that produced a [`BindingError`].
///
/// Listed exhaustively so callers can pattern match and produce a targeted
/// diagnostic for each kind of contract problem.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum BindingErrorKind {
    /// The contract was written with a schema this crate cannot read.
    UnsupportedVersion {
        /// Version found in the serialized contract.
        actual: ContractVersion,
        /// Highest version this crate understands.
        current: ContractVersion,
    },
    /// Two top-level declarations share the same id.
    DuplicateDeclarationId(DeclarationId),
    /// A requested declaration id is not present in the contract.
    UnknownDeclarationId(DeclarationId),
    /// A declaration references an id absent from the contract.
    MissingDeclarationReference {
        /// Declaration that owns the reference.
        owner: DeclarationId,
        /// Referenced declaration that is absent.
        referenced: DeclarationId,
    },
    /// A declaration reference requires a different declaration shape.
    InvalidDeclarationReference {
        /// Declaration that owns the reference.
        owner: DeclarationId,
        /// Referenced declaration with the incompatible shape.
        referenced: DeclarationId,
        /// Shape required by the reference site.
        expected: DeclarationShape,
    },
    /// Two native symbols share the same id.
    DuplicateSymbolId(SymbolId),
    /// Two native symbols share the same exported name.
    DuplicateSymbolName(String),
    /// A native symbol name is empty or not a valid C identifier.
    InvalidSymbolName(String),
    /// A vtable slot name is empty or not a valid Rust identifier.
    InvalidVTableSlot(String),
    /// A wasm import module name is empty.
    InvalidImportModule(String),
    /// A declaration references a native symbol absent from the symbol table.
    UnregisteredSymbol(String),
    /// The symbol table contains a native symbol not referenced by a declaration.
    UnreferencedSymbol(String),
    /// A callable's return shape and error channel both claim the native
    /// return slot.
    ReturnSlotConflict,
    /// A parameter's encoded crossing was tagged `BufferShape::Packed`,
    /// but packing is only meaningful in return position.
    PackedInParamPosition,
    /// A return or error's encoded crossing was tagged
    /// `BufferShape::Slice`, but a borrowed slice cannot be returned to
    /// foreign code with no owner to free it.
    SliceInReturnPosition,
    /// A class requires `Send + Sync` but exposes a `&mut self` method.
    MutableClassReceiverRequiresUnsafeSingleThreaded,
}
