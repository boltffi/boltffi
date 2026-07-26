use boltffi_binding::{Native, SurfaceLower, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

mod async_protocol;
mod buffer;
mod carrier;
mod closure_registration;

pub use async_protocol::AsyncLifecycle;
pub use buffer::BufferCrossings;
pub use carrier::HandleCrossings;
pub use closure_registration::ClosureCrossings;

/// A binding surface that can render generated Rust wrapper items.
///
/// Binding lowering already defines the boundary contract for the surface.
/// Rendering adds the Rust spelling around emitted wrappers. The supertraits
/// of this trait are the complete per-surface render contract: each one
/// resolves the surface's IR values to render lanes or token spellings, and
/// the lane constants name the crossings that are intrinsic to the surface
/// rather than carried by an IR value. A new surface compiles only once
/// every supertrait is implemented for it.
///
/// # Example
///
/// ```rust,ignore
/// let native_cfg = Native::cfg_attr();
/// let wasm_cfg = Wasm32::cfg_attr();
///
/// quote! {
///     #native_cfg
///     #[unsafe(no_mangle)]
///     pub extern "C" fn boltffi_function_demo_add() -> u32 {
///         add()
///     }
///
///     #wasm_cfg
///     #[unsafe(no_mangle)]
///     pub extern "C" fn boltffi_function_demo_add() -> u32 {
///         add()
///     }
/// }
/// ```
pub trait RenderSurface:
    SurfaceLower + BufferCrossings + HandleCrossings + ClosureCrossings + AsyncLifecycle
{
    /// Crossing used for optional scalar parameters and returns.
    const SCALAR_OPTION: ScalarOptionCrossing;

    /// Crossing used for direct record parameters.
    const DIRECT_RECORD_PARAMS: DirectRecordCrossing;

    /// Whether borrowed direct record parameters use typed native pointers.
    const BORROWED_DIRECT_RECORD_PARAMS: bool;

    const INFALLIBLE_VOID_RETURN: InfallibleVoidReturn;

    const STREAM_POLL: StreamPollCrossing;

    /// Returns the `cfg` attribute applied to generated wrapper items for this surface.
    fn cfg_attr() -> TokenStream;
}

/// How an optional scalar crosses the boundary.
#[derive(Clone, Copy)]
pub enum ScalarOptionCrossing {
    /// Wire-encoded option payload carried in a byte buffer.
    WireEncoded,
    /// Single `f64` slot where `NaN` marks the absent value.
    NanBoxedF64,
}

/// How a direct record parameter reaches the wrapper.
#[derive(Clone, Copy)]
pub enum DirectRecordCrossing {
    /// The record value occupies its own parameter slot.
    Value,
    /// A pointer to the record bytes occupies the slot.
    Pointer,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum InfallibleVoidReturn {
    Direct,
    Status,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum StreamPollCrossing {
    Callback,
    WakeImport,
}

impl RenderSurface for Native {
    const SCALAR_OPTION: ScalarOptionCrossing = ScalarOptionCrossing::WireEncoded;
    const DIRECT_RECORD_PARAMS: DirectRecordCrossing = DirectRecordCrossing::Value;
    const BORROWED_DIRECT_RECORD_PARAMS: bool = true;
    const INFALLIBLE_VOID_RETURN: InfallibleVoidReturn = InfallibleVoidReturn::Direct;
    const STREAM_POLL: StreamPollCrossing = StreamPollCrossing::Callback;

    fn cfg_attr() -> TokenStream {
        quote! { #[cfg(not(target_arch = "wasm32"))] }
    }
}

impl RenderSurface for Wasm32 {
    const SCALAR_OPTION: ScalarOptionCrossing = ScalarOptionCrossing::NanBoxedF64;
    const DIRECT_RECORD_PARAMS: DirectRecordCrossing = DirectRecordCrossing::Pointer;
    const BORROWED_DIRECT_RECORD_PARAMS: bool = false;
    const INFALLIBLE_VOID_RETURN: InfallibleVoidReturn = InfallibleVoidReturn::Status;
    const STREAM_POLL: StreamPollCrossing = StreamPollCrossing::WakeImport;

    fn cfg_attr() -> TokenStream {
        quote! { #[cfg(target_arch = "wasm32")] }
    }
}
