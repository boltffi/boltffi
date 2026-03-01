use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// Minimal pull-based stream trait. Mirrors `futures::Stream` without the dependency.
/// Implement this on your type, or use [`stream_from_fn`] for simple cases.
pub trait Stream {
    type Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}

pub type IteratorHandle = *mut core::ffi::c_void;

/// Vtable header stored at offset 0 of every iterator allocation (`repr(C)`).
#[repr(C)]
struct IteratorHeader<T> {
    poll_next: unsafe fn(IteratorHandle, &mut Context<'_>) -> Poll<Option<T>>,
    drop_in_place: unsafe fn(IteratorHandle),
}

/// Single heap allocation: vtable header followed by the concrete stream data.
/// `repr(C)` guarantees `header` sits at offset 0 so a pointer to this struct
/// can be safely cast to `*const IteratorHeader<T>`.
#[repr(C)]
struct IteratorAlloc<T: Send + 'static, S> {
    header: IteratorHeader<T>,
    stream: S,
}

/// Create a new iterator handle from a concrete stream.
///
/// Performs a **single** heap allocation that stores both the vtable and the
/// stream data contiguously.
pub fn iterator_new<T: Send + 'static, S: Stream<Item = T> + Send + 'static>(stream: S) -> IteratorHandle {
    unsafe fn poll_impl<T: Send + 'static, S: Stream<Item = T> + Send + 'static>(handle: IteratorHandle, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let alloc = unsafe { &mut *(handle as *mut IteratorAlloc<T, S>) };
        // SAFETY: The IteratorAlloc lives on the heap (Box) and is never moved
        // after creation, so the stream field is structurally pinned.
        unsafe { Pin::new_unchecked(&mut alloc.stream) }.poll_next(cx)
    }

    unsafe fn drop_impl<T: Send + 'static, S>(handle: IteratorHandle) {
        drop(unsafe { Box::from_raw(handle as *mut IteratorAlloc<T, S>) });
    }

    let alloc = Box::new(IteratorAlloc {
        header: IteratorHeader {
            poll_next: poll_impl::<T, S>,
            drop_in_place: drop_impl::<T, S>,
        },
        stream,
    });
    Box::into_raw(alloc) as IteratorHandle
}

/// Wraps `IteratorHandle` so it can be captured by a `Send` future.
///
/// # Safety
/// The caller must guarantee the pointee is actually safe to send across threads.
struct SendHandle(IteratorHandle);
// SAFETY: IteratorAlloc<T, S> requires T: Send + S: Send, so the data behind
// the handle is Send. The raw pointer itself is just an address; we document
// that callers must not alias it unsafely.
unsafe impl Send for SendHandle {}

impl SendHandle {
    fn get(&self) -> IteratorHandle {
        self.0
    }
}

/// Return a [`Future`] that polls the next item from the iterator behind `handle`.
///
/// # Safety
///
/// `handle` must be a valid pointer previously returned by [`iterator_new`], and
/// the handle must remain valid for the lifetime of the returned future.
pub unsafe fn iterator_next<T: Send + 'static>(handle: IteratorHandle) -> impl std::future::Future<Output = Option<T>> + Send {
    let send_handle = SendHandle(handle);
    std::future::poll_fn(move |cx| {
        let handle = send_handle.get();
        let header = unsafe { &*(handle as *const IteratorHeader<T>) };
        unsafe { (header.poll_next)(handle, cx) }
    })
}

/// Drop the iterator, freeing all resources.
///
/// # Safety
///
/// `handle` must be a valid pointer previously returned by [`iterator_new`] and
/// must not be used again after this call.
pub unsafe fn iterator_free<T: Send + 'static>(handle: IteratorHandle) {
    if !handle.is_null() {
        let header = unsafe { &*(handle as *const IteratorHeader<T>) };
        unsafe { (header.drop_in_place)(handle) };
    }
}
