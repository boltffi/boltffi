use boltffi::{FfiStream, export, ffi_async_iter};
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

pub struct Counter {
    count: i32,
}

struct VecStream<T> {
    items: VecDeque<T>,
}

impl<T> VecStream<T> {
    fn new(items: impl IntoIterator<Item = T>) -> Self {
        Self {
            items: items.into_iter().collect(),
        }
    }
}

impl<T: Send + Unpin + 'static> FfiStream for VecStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<T>> {
        Poll::Ready(self.items.pop_front())
    }
}

#[export(single_threaded)]
impl Counter {
    pub fn new(initial: i32) -> Counter {
        Counter { count: initial }
    }

    pub fn create_with_default() -> Counter {
        Counter { count: 0 }
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }

    pub fn add(&mut self, amount: i32) {
        self.count += amount;
    }

    pub fn get(&self) -> i32 {
        self.count
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub async fn async_add(&mut self, amount: i32) -> i32 {
        self.count += amount;
        self.count
    }

    pub fn transform(&mut self, f: impl Fn(i32) -> i32) -> i32 {
        self.count = f(self.count);
        self.count
    }

    pub fn apply_binary(&self, f: impl Fn(i32, i32) -> i32, other: i32) -> i32 {
        f(self.count, other)
    }

    #[ffi_async_iter(item = i32)]
    pub fn count_up(&self) -> VecStream<i32> {
        let items: Vec<i32> = (1..=self.count).collect();
        VecStream::new(items)
    }
}
