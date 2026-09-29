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

/// Names the tag the `N`th declared type in a custom's representation converts under, so
/// a site can follow a chain of customs across crates.
pub trait CustomReprTag<Tag, const N: usize> {
    /// That declared type's declaring crate's anchor.
    type Tag;
}

/// Names the type argument at position `N` of a representation, so a site can reach a
/// custom type the representation holds.
pub trait ReprArg<const N: usize> {
    /// The argument at that position.
    type Arg;
}

macro_rules! repr_arg {
    ($([$($param:ident),+] $container:ty => $index:literal: $arg:ident;)*) => {
        $(
            impl<$($param),+> ReprArg<$index> for $container {
                type Arg = $arg;
            }
        )*
    };
}

repr_arg!(
    [T] Vec<T> => 0: T;
    [T] Option<T> => 0: T;
    [T] Box<T> => 0: T;
    [T] std::sync::Arc<T> => 0: T;
    [T, E] Result<T, E> => 0: T;
    [T, E] Result<T, E> => 1: E;
    [K, V, S] std::collections::HashMap<K, V, S> => 0: K;
    [K, V, S] std::collections::HashMap<K, V, S> => 1: V;
    [K, V] std::collections::BTreeMap<K, V> => 0: K;
    [K, V] std::collections::BTreeMap<K, V> => 1: V;
    [A] (A,) => 0: A;
    [A, B] (A, B) => 0: A;
    [A, B] (A, B) => 1: B;
    [A, B, C] (A, B, C) => 0: A;
    [A, B, C] (A, B, C) => 1: B;
    [A, B, C] (A, B, C) => 2: C;
);
