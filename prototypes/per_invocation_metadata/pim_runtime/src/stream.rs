//! A pull-based stream. The `#[export]` expansion boxes it behind a `u64` handle and emits
//! the pop and free exports that drain it; async wake semantics are out of scope here.

pub struct PimStream<T> {
    source: Box<dyn FnMut() -> Option<T>>,
}

impl<T> PimStream<T> {
    pub fn from_fn(source: impl FnMut() -> Option<T> + 'static) -> Self {
        Self {
            source: Box::new(source),
        }
    }

    pub fn from_iterator(iterator: impl IntoIterator<Item = T> + 'static) -> Self {
        let mut iterator = iterator.into_iter();
        Self::from_fn(move || iterator.next())
    }

    pub fn pop(&mut self) -> Option<T> {
        (self.source)()
    }
}
