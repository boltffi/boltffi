use std::num::NonZeroUsize;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IdentifierKey(String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NameStem(Vec<String>);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NameOrdinal(NonZeroUsize);

impl IdentifierKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl NameStem {
    pub fn new(part: impl Into<String>) -> Self {
        Self(vec![part.into()])
    }

    pub fn suffixed(mut self, part: impl Into<String>) -> Self {
        self.0.push(part.into());
        self
    }

    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

impl NameOrdinal {
    pub fn get(self) -> usize {
        self.0.get()
    }

    pub(super) fn first() -> Self {
        Self(NonZeroUsize::MIN)
    }

    pub(super) fn next(self) -> Option<Self> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroUsize::new)
            .map(Self)
    }
}
