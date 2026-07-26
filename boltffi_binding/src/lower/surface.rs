//! Per-surface lowering decisions.
//!
//! The IR's [`Surface`] trait defines what each target's
//! divergent shapes *are* — `BufferShape`, `HandleCarrier`,
//! `AsyncProtocol`, `CallbackProtocol`. It does not say which value to
//! pick at a given call site. The lowering pass needs that pick: a
//! string parameter on Wasm32 must cross as `Slice`, a record returned
//! by value on Wasm32 must cross as `Packed`, and so on.
//!
//! [`SurfaceLower`] adds those decisions to a [`Surface`] without
//! touching the IR. It is sealed: only [`Native`] and [`Wasm32`] can
//! implement it. The public lowering function carries the
//! `S: SurfaceLower` bound so external callers cannot supply a surface
//! the lowering pass has not been taught about.
//!
//! [`Surface`]: crate::Surface
//! [`Native`]: crate::Native
//! [`Wasm32`]: crate::Wasm32

use boltffi_ast::FnSig;

use crate::{Native, Primitive, ReturnValueSlot, Surface, Wasm32, native, wasm32};

use super::async_protocol::AsyncProtocolBuilder;
use super::callbacks::CallbackProtocolBuilder;
use super::symbol::SymbolAllocator;
use super::{LowerError, wasm_closure};

mod sealed {
    /// Seals [`super::SurfaceLower`].
    ///
    /// Only the two surfaces shipped with this crate implement it; an
    /// external crate cannot teach the lowering pass to handle a new
    /// surface without changes here.
    pub trait Sealed {}

    impl Sealed for crate::Native {}
    impl Sealed for crate::Wasm32 {}
}

/// A [`Surface`] paired with the lowering-pass decisions that pick its
/// concrete shape values.
///
/// Each method names a fixed call-site role and returns the shape the
/// pass must use there. The choices follow the boltffi convention
/// shared with the foreign-side bindings.
///
/// A private supertrait carries the surface-specific constructor for
/// `Self::CallbackProtocol` so the constructor never appears in this
/// trait's public method set.
#[allow(private_bounds)]
pub trait SurfaceLower:
    Surface + sealed::Sealed + CallbackProtocolBuilder + AsyncProtocolBuilder
{
    /// Buffer shape used for an encoded parameter crossing.
    ///
    /// Encoded params (strings, vecs, encoded records, ...) cross as
    /// pointer-plus-count on every supported surface today.
    #[doc(hidden)]
    fn encoded_param_shape() -> Self::BufferShape;

    /// Buffer shape used for an encoded return crossing.
    ///
    /// Native returns occupy a single descriptor slot
    /// ([`native::BufferShape::Buffer`]). Wasm32 returns occupy one
    /// 64-bit slot folded into [`wasm32::BufferShape::Packed`].
    #[doc(hidden)]
    fn encoded_return_shape() -> Self::BufferShape;

    #[doc(hidden)]
    fn scalar_option(primitive: Primitive) -> Option<Primitive>;

    #[doc(hidden)]
    fn root_string_codec() -> crate::CodecNode;

    #[doc(hidden)]
    fn direct_record_return_slot() -> ReturnValueSlot;

    /// Handle carrier used for a class instance crossing.
    ///
    /// Native classes cross as a 64-bit token
    /// ([`native::HandleCarrier::U64`]). Wasm32 classes cross as a
    /// 32-bit token ([`wasm32::HandleCarrier::U32`]).
    #[doc(hidden)]
    fn class_handle_carrier() -> Self::HandleCarrier;

    /// Handle carrier used for a named callback trait crossing.
    ///
    /// Native callbacks cross through the runtime's
    /// [`native::HandleCarrier::CallbackHandle`] struct so the inner
    /// vtable pointer travels with the handle. Wasm32 callbacks cross
    /// as a 32-bit handle whose vtable dispatch happens through wasm
    /// imports resolved at link time.
    #[doc(hidden)]
    fn callback_handle_carrier() -> Self::HandleCarrier;

    /// Handle carrier used for a stream subscription crossing.
    #[doc(hidden)]
    fn stream_handle_carrier() -> Self::HandleCarrier;

    /// Wire shape used when a foreign-provided closure enters Rust.
    #[doc(hidden)]
    fn incoming_closure_registration(
        closure: &FnSig,
    ) -> Result<Self::IncomingClosureRegistration, LowerError>;

    /// Wire shape used when a Rust-provided closure leaves Rust.
    #[doc(hidden)]
    fn outgoing_closure_registration(
        allocator: &mut SymbolAllocator,
        closure: &FnSig,
    ) -> Result<Self::OutgoingClosureRegistration, LowerError>;
}

impl SurfaceLower for Native {
    fn encoded_param_shape() -> Self::BufferShape {
        native::BufferShape::Slice
    }

    fn encoded_return_shape() -> Self::BufferShape {
        native::BufferShape::Buffer
    }

    fn scalar_option(primitive: Primitive) -> Option<Primitive> {
        Some(primitive)
    }

    fn root_string_codec() -> crate::CodecNode {
        crate::CodecNode::String
    }

    fn direct_record_return_slot() -> ReturnValueSlot {
        ReturnValueSlot::ReturnSlot
    }

    fn class_handle_carrier() -> Self::HandleCarrier {
        native::HandleCarrier::U64
    }

    fn callback_handle_carrier() -> Self::HandleCarrier {
        native::HandleCarrier::CallbackHandle
    }

    fn stream_handle_carrier() -> Self::HandleCarrier {
        native::HandleCarrier::U64
    }

    fn incoming_closure_registration(
        _closure: &FnSig,
    ) -> Result<Self::IncomingClosureRegistration, LowerError> {
        Ok(native::ClosureRegistration::InvokeContextRelease)
    }

    fn outgoing_closure_registration(
        _allocator: &mut SymbolAllocator,
        _closure: &FnSig,
    ) -> Result<Self::OutgoingClosureRegistration, LowerError> {
        Ok(native::ClosureRegistration::InvokeContextRelease)
    }
}

impl SurfaceLower for Wasm32 {
    fn encoded_param_shape() -> Self::BufferShape {
        wasm32::BufferShape::Slice
    }

    fn encoded_return_shape() -> Self::BufferShape {
        wasm32::BufferShape::Packed
    }

    fn scalar_option(primitive: Primitive) -> Option<Primitive> {
        matches!(
            primitive,
            Primitive::Bool
                | Primitive::I8
                | Primitive::U8
                | Primitive::I16
                | Primitive::U16
                | Primitive::I32
                | Primitive::U32
                | Primitive::ISize
                | Primitive::USize
                | Primitive::F64
        )
        .then_some(primitive)
    }

    fn root_string_codec() -> crate::CodecNode {
        crate::CodecNode::Utf8String
    }

    fn direct_record_return_slot() -> ReturnValueSlot {
        ReturnValueSlot::OutPointer
    }

    fn class_handle_carrier() -> Self::HandleCarrier {
        wasm32::HandleCarrier::U32
    }

    fn callback_handle_carrier() -> Self::HandleCarrier {
        wasm32::HandleCarrier::U32
    }

    fn stream_handle_carrier() -> Self::HandleCarrier {
        wasm32::HandleCarrier::U32
    }

    fn incoming_closure_registration(
        closure: &FnSig,
    ) -> Result<Self::IncomingClosureRegistration, LowerError> {
        wasm_closure::incoming_registration(closure)
    }

    fn outgoing_closure_registration(
        allocator: &mut SymbolAllocator,
        closure: &FnSig,
    ) -> Result<Self::OutgoingClosureRegistration, LowerError> {
        wasm_closure::outgoing_registration(allocator, closure)
    }
}
