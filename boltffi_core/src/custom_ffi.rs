use crate::wire::{WireDecode, WireEncode};

#[allow(clippy::wrong_self_convention)]
pub trait CustomFfiConvertible: Sized {
    type FfiRepr: WireEncode + WireDecode;
    type Error;

    fn into_ffi(&self) -> Self::FfiRepr;
    fn try_from_ffi(repr: Self::FfiRepr) -> Result<Self, Self::Error>;
}

/// Conversions a `custom_type!` registration supplies for a remote type.
///
/// `Tag` is the registering crate's anchor, which keeps the impl inside the orphan rule.
pub trait CustomType<Tag> {
    /// The representation that crosses the boundary.
    type Repr;
    /// The error a failed conversion from the representation reports.
    type Error;

    /// Converts the remote value into its representation.
    fn into_ffi(value: &Self) -> Self::Repr;

    /// Rebuilds the remote value from its representation.
    fn try_from_ffi(repr: Self::Repr) -> Result<Self, Self::Error>
    where
        Self: Sized;
}
