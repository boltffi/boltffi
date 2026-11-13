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

/// The type argument at position `N` of `Container`, written as a path.
///
/// A site names a declared type that a type alias holds inside a container this way, since
/// the alias spells the container but not what it holds.
///
/// # Example
///
/// With `type Points = Vec<Point>`, `ReprArgOf<Points, 0>` is `Point`.
pub type ReprArgOf<Container, const N: usize> = <Container as ReprArg<N>>::Arg;

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
);

macro_rules! tuple_repr_arg {
    ($($param:ident),+) => {
        tuple_repr_arg!(@at [$($param),+] (0) $($param)+);
    };
    (@at [$($param:ident),+] ($index:expr) $arg:ident $($rest:ident)*) => {
        impl<$($param),+> ReprArg<{ $index }> for ($($param,)+) {
            type Arg = $arg;
        }
        tuple_repr_arg!(@at [$($param),+] ($index + 1) $($rest)*);
    };
    (@at [$($param:ident),+] ($index:expr)) => {};
}

tuple_repr_arg!(A);
tuple_repr_arg!(A, B);
tuple_repr_arg!(A, B, C);
tuple_repr_arg!(A, B, C, D);
tuple_repr_arg!(A, B, C, D, E);
tuple_repr_arg!(A, B, C, D, E, F);
tuple_repr_arg!(A, B, C, D, E, F, G);
tuple_repr_arg!(A, B, C, D, E, F, G, H);
tuple_repr_arg!(A, B, C, D, E, F, G, H, I);
tuple_repr_arg!(A, B, C, D, E, F, G, H, I, J);
tuple_repr_arg!(A, B, C, D, E, F, G, H, I, J, K);
tuple_repr_arg!(A, B, C, D, E, F, G, H, I, J, K, L);

#[cfg(test)]
mod tests {
    use super::{ReprArg, ReprArgOf};

    fn arg_of<Repr: ReprArg<N>, const N: usize>() -> &'static str {
        std::any::type_name::<<Repr as ReprArg<N>>::Arg>()
    }

    #[test]
    fn every_tuple_position_names_its_argument() {
        type Wide = (u8, u16, u32, u64, i8, i16, i32, i64, f32, f64, bool, char);

        assert_eq!(arg_of::<(u8, u16, u32, i64), 3>(), "i64");
        assert_eq!(arg_of::<Wide, 0>(), "u8");
        assert_eq!(arg_of::<Wide, 7>(), "i64");
        assert_eq!(arg_of::<Wide, 11>(), "char");
    }

    #[test]
    fn an_alias_names_what_its_container_holds() {
        type Readings = Vec<Option<(u8, String)>>;
        type Label = ReprArgOf<ReprArgOf<ReprArgOf<Readings, 0>, 0>, 1>;

        assert_eq!(
            std::any::type_name::<Label>(),
            std::any::type_name::<String>()
        );
    }
}
