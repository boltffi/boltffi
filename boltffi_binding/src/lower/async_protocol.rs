//! Async protocol lowering.
//!
//! Async callables describe the same eventual value as their sync
//! peers (the [`CallableDecl`]'s `returns` and `error` are what
//! `complete_*` yields once the operation is ready). The async-ness
//! lives entirely on the [`ExecutionDecl::Asynchronous`] axis, which
//! carries the surface's chosen [`Surface::AsyncProtocol`] value with
//! the lifecycle symbols foreign code drives.
//!
//! Both supported surfaces ship the `PollHandle` protocol today. The
//! other native variants ([`native::AsyncProtocol::NativeFuture`],
//! [`native::AsyncProtocol::Continuation`]) live in the IR for future
//! use but the lowering pass never produces them.
//!
//! The protocol's symbol names are derived from the start callable's
//! own symbol so every async operation's symbols group together when
//! grepped: a method `compute` on `demo::Engine` adds
//! `boltffi_async_method_record_demo_engine_compute_poll`, and so on.
//!
//! [`CallableDecl`]: crate::CallableDecl
//! [`ExecutionDecl::Asynchronous`]: crate::ExecutionDecl::Asynchronous
//! [`Surface::AsyncProtocol`]: crate::Surface::AsyncProtocol
//! [`native::AsyncProtocol::NativeFuture`]: crate::native::AsyncProtocol::NativeFuture
//! [`native::AsyncProtocol::Continuation`]: crate::native::AsyncProtocol::Continuation

use crate::{Native, NativeSymbol, Surface, Wasm32, native, wasm32};

use super::LowerError;
use super::symbol::{AsyncLifecycle, SymbolAllocator};

/// Surface-specific construction of [`Surface::AsyncProtocol`].
///
/// Implemented for [`Native`] and [`Wasm32`] only. Wired in as a
/// private supertrait of [`super::surface::SurfaceLower`] so the public
/// lowering API stays a shape-picker contract; the protocol
/// constructor is reachable only through the sealed bound.
///
/// `start_symbol_name` is the symbol foreign code calls to begin the
/// operation. The builder mints every lifecycle symbol with that name
/// as the prefix so an async callable's full symbol set is contiguous
/// in the symbol table.
pub trait AsyncProtocolBuilder: Surface {
    fn build_protocol(
        allocator: &mut SymbolAllocator,
        start_symbol_name: &str,
    ) -> Result<Self::AsyncProtocol, LowerError>;
}

impl AsyncProtocolBuilder for Native {
    fn build_protocol(
        allocator: &mut SymbolAllocator,
        start_symbol_name: &str,
    ) -> Result<Self::AsyncProtocol, LowerError> {
        let poll = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Poll)?;
        let complete = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Complete)?;
        let cancel = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Cancel)?;
        let free = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Free)?;
        let panic_message = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Panic)?;
        Ok(native::AsyncProtocol::PollHandle {
            handle: native::HandleCarrier::U64,
            poll,
            complete,
            cancel,
            free,
            panic_message,
        })
    }
}

impl AsyncProtocolBuilder for Wasm32 {
    fn build_protocol(
        allocator: &mut SymbolAllocator,
        start_symbol_name: &str,
    ) -> Result<Self::AsyncProtocol, LowerError> {
        let poll_sync = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::PollSync)?;
        let complete = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Complete)?;
        let cancel = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Cancel)?;
        let free = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Free)?;
        let panic_message = mint_lifecycle(allocator, start_symbol_name, AsyncLifecycle::Panic)?;
        Ok(wasm32::AsyncProtocol::PollHandle {
            handle: wasm32::HandleCarrier::U32,
            poll_sync,
            complete,
            cancel,
            free,
            panic_message,
        })
    }
}

fn mint_lifecycle(
    allocator: &mut SymbolAllocator,
    start_symbol_name: &str,
    action: AsyncLifecycle,
) -> Result<NativeSymbol, LowerError> {
    allocator.mint_async_lifecycle(start_symbol_name, action)
}
