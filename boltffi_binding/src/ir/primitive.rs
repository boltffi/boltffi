use serde::{Deserialize, Serialize};

use super::{layout::ByteSize, surface::Surface};

/// The set of scalar values that can cross the FFI boundary directly.
///
/// These are the shapes whose ABI representation is the same on both sides:
/// `i32` is four bytes here and four bytes there, `f64` follows IEEE-754 in
/// either direction. Higher-level Rust types like `String`, `Vec<T>`, or
/// user records are not primitives; they require encoding, ownership, or
/// layout work to cross.
///
/// Source aliases (a `u32` written as `MyHandle` in Rust) are resolved
/// before a value reaches this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Primitive {
    /// Rust `bool`.
    Bool,
    /// Rust `i8`.
    I8,
    /// Rust `u8`.
    U8,
    /// Rust `i16`.
    I16,
    /// Rust `u16`.
    U16,
    /// Rust `i32`.
    I32,
    /// Rust `u32`.
    U32,
    /// Rust `i64`.
    I64,
    /// Rust `u64`.
    U64,
    /// Rust `isize`.
    ISize,
    /// Rust `usize`.
    USize,
    /// Rust `f32`.
    F32,
    /// Rust `f64`.
    F64,
}

impl Primitive {
    /// Returns the ABI byte width of this primitive.
    pub const fn byte_size<S: Surface>(self) -> ByteSize {
        ByteSize::new(match self {
            Self::Bool | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
            Self::ISize | Self::USize => S::POINTER_SIZE.get(),
        })
    }

    /// Returns the wire byte width of this primitive.
    ///
    /// Wire scalars are surface-independent: platform-width integers
    /// always occupy eight bytes on the wire regardless of the pointer
    /// size of the surface that produced them.
    pub const fn wire_size(self) -> ByteSize {
        ByteSize::new(match self {
            Self::Bool | Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 | Self::ISize | Self::USize => 8,
        })
    }
}

/// The integer-sized scalars allowed for enum tags and status codes.
///
/// A subset of [`Primitive`] that excludes `bool`, `f32`, and `f64`.
/// Selecting an enum discriminant or a status return through a separate
/// type means the type system rejects "tag as `bool`" or "status as `f32`"
/// at the call site rather than at validation.
///
/// # Example
///
/// A C-style enum tagged as `i32` is represented by `IntegerRepr::I32`.
/// `primitive()` converts it to its scalar carrier, `Primitive::I32`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IntegerRepr {
    /// Rust `i8`.
    I8,
    /// Rust `u8`.
    U8,
    /// Rust `i16`.
    I16,
    /// Rust `u16`.
    U16,
    /// Rust `i32`.
    I32,
    /// Rust `u32`.
    U32,
    /// Rust `i64`.
    I64,
    /// Rust `u64`.
    U64,
    /// Rust `isize`.
    ISize,
    /// Rust `usize`.
    USize,
}

impl IntegerRepr {
    /// Returns the matching [`Primitive`].
    pub const fn primitive(self) -> Primitive {
        match self {
            Self::I8 => Primitive::I8,
            Self::U8 => Primitive::U8,
            Self::I16 => Primitive::I16,
            Self::U16 => Primitive::U16,
            Self::I32 => Primitive::I32,
            Self::U32 => Primitive::U32,
            Self::I64 => Primitive::I64,
            Self::U64 => Primitive::U64,
            Self::ISize => Primitive::ISize,
            Self::USize => Primitive::USize,
        }
    }
}

impl From<IntegerRepr> for Primitive {
    fn from(repr: IntegerRepr) -> Self {
        repr.primitive()
    }
}
