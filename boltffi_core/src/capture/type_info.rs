/// Canonical identity of a named FFI type, implemented at its `#[data]`/`custom_type!` site.
///
/// `Tag` is the referencing crate's anchor type, which is what keeps remote registrations
/// inside the orphan rule.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no canonical id, so it cannot cross the FFI boundary",
    label = "no canonical id",
    note = "annotate `{Self}` with `#[data]`, or declare it with `custom_type!` if it is foreign"
)]
pub trait TypeInfo<Tag> {
    /// The defining module path, as `module_path!()` sees it at the definition site.
    const MODULE: &'static str;
    /// The type name at the definition site.
    const NAME: &'static str;
}

/// Structural descriptor of a type at an FFI use site, composable in const context so
/// aliases to containers resolve through the compiler.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no canonical id, so it cannot cross the FFI boundary",
    label = "no canonical id",
    note = "annotate `{Self}` with `#[data]`, or declare it with `custom_type!` if it is foreign"
)]
pub trait TypeDesc<Tag> {
    /// One JSON type node: `{"id":..}`, `{"prim":..}`, or `{"shape":..,"args":[..]}`.
    const DESC: DescBuf;
}

/// Capacity of a [`DescBuf`]; deep nesting past this is a compile error.
pub const DESC_CAPACITY: usize = 1024;

/// A fixed-capacity const string buffer holding one composed type descriptor.
#[derive(Clone, Copy)]
pub struct DescBuf {
    bytes: [u8; DESC_CAPACITY],
    len: usize,
}

impl DescBuf {
    /// Creates an empty buffer.
    pub const fn new() -> Self {
        Self {
            bytes: [0; DESC_CAPACITY],
            len: 0,
        }
    }

    /// Builds the node for a named type from its [`TypeInfo`] constants.
    pub const fn named(module: &str, name: &str) -> Self {
        Self::new()
            .push(b"{\"id\":\"")
            .push_str(module)
            .push(b"::")
            .push_str(name)
            .push(b"\"}")
    }

    /// Builds the node for a primitive or builtin leaf type.
    pub const fn primitive(name: &str) -> Self {
        Self::new()
            .push(b"{\"prim\":\"")
            .push_str(name)
            .push(b"\"}")
    }

    /// Builds the node for a one-argument container shape.
    pub const fn shape_with_one_arg(name: &str, arg: Self) -> Self {
        Self::shape_open(name).concat(arg).push(b"]}")
    }

    /// Builds the node for a two-argument container shape.
    pub const fn shape_with_two_args(name: &str, first: Self, second: Self) -> Self {
        Self::shape_open(name)
            .concat(first)
            .push(b",")
            .concat(second)
            .push(b"]}")
    }

    const fn shape_open(name: &str) -> Self {
        Self::new()
            .push(b"{\"shape\":\"")
            .push_str(name)
            .push(b"\",\"args\":[")
    }

    const fn push(mut self, source: &[u8]) -> Self {
        assert!(
            self.len + source.len() <= DESC_CAPACITY,
            "type descriptor exceeds the capture buffer"
        );

        let mut index = 0;
        while index < source.len() {
            self.bytes[self.len + index] = source[index];
            index += 1;
        }
        self.len += source.len();
        self
    }

    const fn push_str(self, value: &str) -> Self {
        self.push(value.as_bytes())
    }

    const fn concat(self, other: Self) -> Self {
        self.push(other.as_bytes())
    }

    /// Returns the composed descriptor bytes.
    pub const fn as_bytes(&self) -> &[u8] {
        self.bytes.split_at(self.len).0
    }

    /// Returns the composed descriptor as a string.
    pub const fn as_str(&self) -> &str {
        match str::from_utf8(self.as_bytes()) {
            Ok(value) => value,
            Err(_) => panic!("type descriptor is not utf-8"),
        }
    }
}

impl Default for DescBuf {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! primitive_desc {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<Tag> TypeDesc<Tag> for $ty {
                const DESC: DescBuf = DescBuf::primitive(stringify!($ty));
            }
        )*
    };
}

primitive_desc!(
    bool, u8, u16, u32, u64, i8, i16, i32, i64, usize, isize, f32, f64
);

impl<Tag> TypeDesc<Tag> for String {
    const DESC: DescBuf = DescBuf::primitive("String");
}

impl<Tag, T: TypeDesc<Tag>> TypeDesc<Tag> for Vec<T> {
    const DESC: DescBuf = DescBuf::shape_with_one_arg("Vec", T::DESC);
}

impl<Tag, T: TypeDesc<Tag>> TypeDesc<Tag> for Option<T> {
    const DESC: DescBuf = DescBuf::shape_with_one_arg("Option", T::DESC);
}

impl<Tag, T: TypeDesc<Tag>> TypeDesc<Tag> for Box<T> {
    const DESC: DescBuf = DescBuf::shape_with_one_arg("Box", T::DESC);
}

impl<Tag, T: TypeDesc<Tag>> TypeDesc<Tag> for std::sync::Arc<T> {
    const DESC: DescBuf = DescBuf::shape_with_one_arg("Arc", T::DESC);
}

impl<Tag, K: TypeDesc<Tag>, V: TypeDesc<Tag>> TypeDesc<Tag> for std::collections::HashMap<K, V> {
    const DESC: DescBuf = DescBuf::shape_with_two_args("HashMap", K::DESC, V::DESC);
}

impl<Tag, K: TypeDesc<Tag>, V: TypeDesc<Tag>> TypeDesc<Tag> for std::collections::BTreeMap<K, V> {
    const DESC: DescBuf = DescBuf::shape_with_two_args("BTreeMap", K::DESC, V::DESC);
}

impl<Tag, T: TypeDesc<Tag>, E: TypeDesc<Tag>> TypeDesc<Tag> for Result<T, E> {
    const DESC: DescBuf = DescBuf::shape_with_two_args("Result", T::DESC, E::DESC);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{record, record_len};

    struct ProbeTag;

    struct Point;

    impl<Tag> TypeInfo<Tag> for Point {
        const MODULE: &'static str = "demo::geometry";
        const NAME: &'static str = "Point";
    }

    impl<Tag> TypeDesc<Tag> for Point {
        const DESC: DescBuf = DescBuf::named(
            <Point as TypeInfo<Tag>>::MODULE,
            <Point as TypeInfo<Tag>>::NAME,
        );
    }

    type Points = Vec<Point>;

    #[test]
    fn composes_a_named_leaf() {
        const DESC: &DescBuf = &<Point as TypeDesc<ProbeTag>>::DESC;
        assert_eq!(
            DESC.as_str(),
            r#"{"id":"demo::geometry::Point"}"#,
            "named types carry their canonical id"
        );
    }

    #[test]
    fn composes_through_container_aliases() {
        const DESC: &DescBuf = &<Points as TypeDesc<ProbeTag>>::DESC;
        assert_eq!(
            DESC.as_str(),
            r#"{"shape":"Vec","args":[{"id":"demo::geometry::Point"}]}"#,
            "an alias to a container resolves to the container's structure"
        );
    }

    #[test]
    fn composes_nested_two_argument_shapes() {
        const DESC: &DescBuf = &<HashMapAlias as TypeDesc<ProbeTag>>::DESC;
        type HashMapAlias = std::collections::HashMap<String, Option<Point>>;
        assert_eq!(
            DESC.as_str(),
            concat!(
                r#"{"shape":"HashMap","args":[{"prim":"String"},"#,
                r#"{"shape":"Option","args":[{"id":"demo::geometry::Point"}]}]}"#
            ),
            "two-argument shapes separate their arguments"
        );
    }

    #[test]
    fn descriptor_strings_promote_to_static_slots() {
        const DESC: &DescBuf = &<Points as TypeDesc<ProbeTag>>::DESC;
        const SLOT: &str = DESC.as_str();
        const SLOTS: &[&str] = &[SLOT];
        const LEN: usize = record_len("p", "", "m", SLOTS, b"{}");
        static RECORD: [u8; LEN] = record("p", "", "m", SLOTS, b"{}");
        assert!(
            RECORD.len() > SLOT.len(),
            "a record embeds the promoted descriptor"
        );
    }
}
