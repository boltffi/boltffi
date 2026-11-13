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
///
/// Every type has exactly one descriptor. `Tag` is the type itself when its crate declares it,
/// the declaring crate's anchor for a custom type, `()` for a primitive, and the tag of the
/// parts for a container, so a use site reads `<T as TypeDesc<_>>::DESC` and the compiler
/// selects the declaration however `T` is spelled, including through aliases and paths into
/// other crates.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no canonical id, so it cannot cross the FFI boundary",
    label = "no canonical id",
    note = "annotate `{Self}` with `#[data]`, or declare it with `custom_type!` if it is foreign"
)]
pub trait TypeDesc<Tag> {
    /// One JSON type node: `{"id":..,"fingerprint":..}`, `{"prim":..}`, or
    /// `{"shape":..,"args":[..]}`.
    const DESC: DescBuf;
}

/// Panics with `message` unless `found` and `expected` are the same descriptor.
///
/// Called in const context by wrappers expanded from prepared records, so a spelling that
/// names another declaration, or a declaration that changed since the records were
/// prepared, fails the build instead of expanding a wrapper for the old contract.
pub const fn expect_descriptor(found: &str, expected: &str, message: &str) {
    let (found, expected) = (found.as_bytes(), expected.as_bytes());
    let mut same = found.len() == expected.len();
    let mut index = 0;
    while same && index < found.len() {
        same = found[index] == expected[index];
        index += 1;
    }
    if !same {
        panic!("{}", message);
    }
}

/// Descriptor of an exported class, implemented by the `#[export]` struct or enum that
/// declares it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an exported class",
    label = "an `#[export] impl` block adds methods to an exported class",
    note = "declare the class with `#[export]` on the struct or enum `{Self}`"
)]
pub trait ClassDesc<Tag> {
    /// One JSON type node naming the class.
    const DESC: DescBuf;
}

/// Descriptor of a callback trait, carried by the marker value its export site defines
/// under the trait's own name.
pub trait TraitDesc {
    /// One JSON type node naming the trait.
    const DESC: DescBuf;
}

/// Reads the descriptor of the trait whose marker value `marker` is.
pub const fn trait_desc<M: TraitDesc>(_marker: &M) -> DescBuf {
    M::DESC
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

    /// Builds the node for a declared type from its [`TypeInfo`] constants and the
    /// fingerprint of the fragment that declares it.
    pub const fn declared(module: &str, name: &str, fingerprint: &str) -> Self {
        Self::declared_open(module, name, fingerprint).push(b"}")
    }

    /// Builds the node for a declared type whose crossing also reads the declarations in
    /// `uses`, such as the representation of a custom type.
    pub const fn declared_using(
        module: &str,
        name: &str,
        fingerprint: &str,
        uses: &[DescBuf],
    ) -> Self {
        let mut desc = Self::declared_open(module, name, fingerprint).push(b",\"uses\":[");
        let mut index = 0;
        while index < uses.len() {
            if index > 0 {
                desc = desc.push(b",");
            }
            desc = desc.concat(uses[index]);
            index += 1;
        }
        desc.push(b"]}")
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

    const fn declared_open(module: &str, name: &str, fingerprint: &str) -> Self {
        Self::new()
            .push(b"{\"id\":\"")
            .push_str(module)
            .push(b"::")
            .push_str(name)
            .push(b"\",\"fingerprint\":\"")
            .push_str(fingerprint)
            .push(b"\"")
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
            impl TypeDesc<()> for $ty {
                const DESC: DescBuf = DescBuf::primitive(stringify!($ty));
            }
        )*
    };
}

primitive_desc!(
    bool, u8, u16, u32, u64, i8, i16, i32, i64, usize, isize, f32, f64
);

impl TypeDesc<()> for String {
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

impl<KeyTag, ValueTag, K: TypeDesc<KeyTag>, V: TypeDesc<ValueTag>> TypeDesc<(KeyTag, ValueTag)>
    for std::collections::HashMap<K, V>
{
    const DESC: DescBuf = DescBuf::shape_with_two_args("HashMap", K::DESC, V::DESC);
}

impl<KeyTag, ValueTag, K: TypeDesc<KeyTag>, V: TypeDesc<ValueTag>> TypeDesc<(KeyTag, ValueTag)>
    for std::collections::BTreeMap<K, V>
{
    const DESC: DescBuf = DescBuf::shape_with_two_args("BTreeMap", K::DESC, V::DESC);
}

impl<OkTag, ErrTag, T: TypeDesc<OkTag>, E: TypeDesc<ErrTag>> TypeDesc<(OkTag, ErrTag)>
    for Result<T, E>
{
    const DESC: DescBuf = DescBuf::shape_with_two_args("Result", T::DESC, E::DESC);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{record, record_len};

    struct UnitsTag;

    struct Point;

    impl<Tag> TypeInfo<Tag> for Point {
        const MODULE: &'static str = "demo::geometry";
        const NAME: &'static str = "Point";
    }

    impl TypeDesc<Point> for Point {
        const DESC: DescBuf = DescBuf::declared(
            <Point as TypeInfo<Point>>::MODULE,
            <Point as TypeInfo<Point>>::NAME,
            "0a1b",
        );
    }

    struct Meters;

    impl TypeDesc<UnitsTag> for Meters {
        const DESC: DescBuf = DescBuf::declared_using(
            "demo::units",
            "Meters",
            "2c3d",
            &[<Point as TypeDesc<_>>::DESC, <u32 as TypeDesc<_>>::DESC],
        );
    }

    type Points = Vec<Point>;

    #[test]
    fn composes_a_declared_leaf() {
        const DESC: &DescBuf = &<Point as TypeDesc<_>>::DESC;
        assert_eq!(
            DESC.as_str(),
            r#"{"id":"demo::geometry::Point","fingerprint":"0a1b"}"#,
            "declared types carry their canonical id and fingerprint"
        );
    }

    #[test]
    fn composes_the_declarations_a_custom_reads() {
        const DESC: &DescBuf = &<Meters as TypeDesc<_>>::DESC;
        assert_eq!(
            DESC.as_str(),
            concat!(
                r#"{"id":"demo::units::Meters","fingerprint":"2c3d","uses":["#,
                r#"{"id":"demo::geometry::Point","fingerprint":"0a1b"},{"prim":"u32"}]}"#
            ),
            "a custom's descriptor changes with the declarations its representation names"
        );
    }

    #[test]
    fn composes_through_container_aliases() {
        const DESC: &DescBuf = &<Points as TypeDesc<_>>::DESC;
        assert_eq!(
            DESC.as_str(),
            r#"{"shape":"Vec","args":[{"id":"demo::geometry::Point","fingerprint":"0a1b"}]}"#,
            "an alias to a container resolves to the container's structure"
        );
    }

    #[test]
    fn matching_descriptors_pass_the_resolution_check() {
        const DESC: &DescBuf = &<Point as TypeDesc<_>>::DESC;
        const _: () = expect_descriptor(
            DESC.as_str(),
            r#"{"id":"demo::geometry::Point","fingerprint":"0a1b"}"#,
            "unchanged",
        );
    }

    #[test]
    #[should_panic(expected = "`Distance` names a different type")]
    fn a_changed_descriptor_fails_the_resolution_check() {
        expect_descriptor(
            r#"{"id":"demo::geometry::Point","fingerprint":"9f9f"}"#,
            r#"{"id":"demo::geometry::Point","fingerprint":"0a1b"}"#,
            "`Distance` names a different type",
        );
    }

    #[test]
    fn composes_nested_two_argument_shapes() {
        const DESC: &DescBuf = &<HashMapAlias as TypeDesc<_>>::DESC;
        type HashMapAlias = std::collections::HashMap<String, Option<Point>>;
        assert_eq!(
            DESC.as_str(),
            concat!(
                r#"{"shape":"HashMap","args":[{"prim":"String"},"#,
                r#"{"shape":"Option","args":[{"id":"demo::geometry::Point","fingerprint":"0a1b"}]}]}"#
            ),
            "two-argument shapes separate their arguments"
        );
    }

    #[test]
    fn composes_declarations_from_different_crates() {
        const DESC: &DescBuf = &<Result<Point, Meters> as TypeDesc<_>>::DESC;
        assert_eq!(
            DESC.as_str(),
            concat!(
                r#"{"shape":"Result","args":[{"id":"demo::geometry::Point","fingerprint":"0a1b"},"#,
                r#"{"id":"demo::units::Meters","fingerprint":"2c3d","uses":["#,
                r#"{"id":"demo::geometry::Point","fingerprint":"0a1b"},{"prim":"u32"}]}]}"#
            ),
            "each part keeps the descriptor of the crate that declares it"
        );
    }

    #[test]
    fn descriptor_strings_promote_to_static_slots() {
        const DESC: &DescBuf = &<Points as TypeDesc<_>>::DESC;
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
