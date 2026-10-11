//! The escalation from `MODULE`/`NAME` to a composable descriptor: a fixed-capacity const buffer
//! that blanket impls can concatenate, so containers and aliases resolve without a macro vocabulary.

pub const CAPACITY: usize = 256;

#[derive(Clone, Copy)]
pub struct Meta {
    bytes: [u8; CAPACITY],
    len: usize,
}

impl Meta {
    pub const fn new() -> Self {
        Self {
            bytes: [0; CAPACITY],
            len: 0,
        }
    }

    pub const fn push(mut self, source: &[u8]) -> Self {
        assert!(
            self.len + source.len() <= CAPACITY,
            "type descriptor exceeds the metadata buffer"
        );

        let mut index = 0;
        while index < source.len() {
            self.bytes[self.len + index] = source[index];
            index += 1;
        }
        self.len += source.len();
        self
    }

    pub const fn concat(self, other: Meta) -> Self {
        self.push(other.as_bytes())
    }

    pub const fn as_bytes(&self) -> &[u8] {
        self.bytes.split_at(self.len).0
    }

    pub const fn as_str(&self) -> &str {
        match str::from_utf8(self.as_bytes()) {
            Ok(value) => value,
            Err(_) => panic!("type descriptor is not utf-8"),
        }
    }
}

impl Default for Meta {
    fn default() -> Self {
        Self::new()
    }
}

/// Like [`TypeInfo`](crate::TypeInfo), but the id composes, so containers get blanket impls.
pub trait TypeMeta<Tag> {
    const META: Meta;
}

macro_rules! primitive {
    ($($ty:ty),*) => {
        $(
            impl<Tag> TypeMeta<Tag> for $ty {
                const META: Meta = Meta::new().push(stringify!($ty).as_bytes());
            }
        )*
    };
}

primitive!(
    bool, char, String, u8, u16, u32, u64, i8, i16, i32, i64, f32, f64
);

impl<Tag, T: TypeMeta<Tag>> TypeMeta<Tag> for Vec<T> {
    const META: Meta = Meta::new().push(b"Vec<").concat(T::META).push(b">");
}

impl<Tag, T: TypeMeta<Tag>> TypeMeta<Tag> for Option<T> {
    const META: Meta = Meta::new().push(b"Option<").concat(T::META).push(b">");
}

impl<Tag, T: TypeMeta<Tag>> TypeMeta<Tag> for Box<T> {
    const META: Meta = Meta::new().push(b"Box<").concat(T::META).push(b">");
}

impl<Tag, K: TypeMeta<Tag>, V: TypeMeta<Tag>> TypeMeta<Tag> for std::collections::HashMap<K, V> {
    const META: Meta = Meta::new()
        .push(b"HashMap<")
        .concat(K::META)
        .push(b", ")
        .concat(V::META)
        .push(b">");
}
