use std::sync::Arc;

use super::{ArcFromCallbackHandle, BoxFromCallbackHandle, CallbackHandle};

/// Associates a Rust callback-facing type with its generated foreign wrapper.
///
/// The `Foreign` type is the wrapper that actually crosses the FFI boundary.
/// It must support whichever ownership recovery modes the generated callback
/// glue requires.
pub trait CallbackForeignType {
    /// Generated wrapper type that is passed across the FFI boundary.
    ///
    /// Implementations choose the foreign wrapper that matches the Rust-facing
    /// callback type. That wrapper must be able to rebuild both shared and
    /// unique ownership from a raw [`super::CallbackHandle`] because generated
    /// callback glue may need either recovery mode depending on how the
    /// callback is used.
    type Foreign: ArcFromCallbackHandle + BoxFromCallbackHandle;
}

/// Hands a Rust implementation of a callback trait to the foreign side.
///
/// Implemented for `dyn Trait` at the trait's export site, so any use site reaches it
/// through the trait's own path.
pub trait CallbackLocalHandle {
    /// Registers `callback` and returns the handle the foreign side calls it through.
    fn local_callback_handle(callback: Arc<Self>) -> CallbackHandle;
}

/// Links an exported class to the handle type its export site generates.
pub trait ClassHandle {
    /// The generated handle type that owns one class instance across the boundary.
    type Handle;
}

/// Links a callback trait's marker value to the foreign proxy its export site generates.
///
/// The marker value lives under the trait's own name, so a use site reaches the proxy
/// through the path it already wrote for the trait, whether or not the trait is dyn
/// compatible.
pub trait CallbackMarker {
    /// The generated proxy that forwards trait calls to the foreign side.
    type Foreign: ArcFromCallbackHandle + BoxFromCallbackHandle;
}

/// Rebuilds the boxed foreign proxy for the trait `marker` names.
///
/// # Safety
///
/// `handle` must be a live owned handle created for this trait's proxy.
pub unsafe fn callback_box<M: CallbackMarker>(
    _marker: M,
    handle: CallbackHandle,
) -> Box<M::Foreign> {
    unsafe { M::Foreign::box_from_callback_handle(handle) }
}

/// Rebuilds the shared foreign proxy for the trait `marker` names.
///
/// # Safety
///
/// `handle` must be a live handle created for this trait's proxy.
pub unsafe fn callback_arc<M: CallbackMarker>(
    _marker: M,
    handle: CallbackHandle,
) -> Arc<M::Foreign> {
    unsafe { M::Foreign::arc_from_callback_handle(handle) }
}
