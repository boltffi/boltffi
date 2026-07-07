//! Per-target call-surface definitions for the binding IR.
//!
//! A binding contract is parameterized by a [`Surface`]: the concrete
//! target whose ABI the contract describes. Each surface picks its own
//! shape for the target-divergent concepts the IR carries:
//!
//! - the protocol foreign code uses to install and dispatch a callback
//!   trait,
//! - the layout of an encoded buffer across native call slots,
//! - the carrier that names the integer or struct used for an opaque
//!   handle,
//! - the registration surface used for inline closures,
//! - the protocol used to drive an asynchronous callable to completion.
//!
//! The trait associates each of these to one concrete type per surface;
//! everything in [`crate::ir`] that names a target-divergent concept
//! reads it through `S: Surface`. A `Bindings<Native>` cannot pass for a
//! `Bindings<Wasm32>`, and a Swift backend typed against
//! `Bindings<Native>` cannot accidentally receive a wasm contract.
//!
//! # Example
//!
//! ```ignore
//! use boltffi_binding::{Bindings, Native, Wasm32};
//!
//! fn render_native(bindings: &Bindings<Native>) { /* ... */ }
//! fn render_wasm  (bindings: &Bindings<Wasm32>) { /* ... */ }
//! ```

use std::fmt::Debug;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::{Bindings, ByteSize, ImportedCallable, NativeSymbol, SerializedBindings};

/// A target whose call surface a binding contract describes.
///
/// Implementors are zero-sized markers carrying no value at run time.
/// Each implementor binds one concrete type to every target-divergent
/// IR concept; downstream code reads the concept through
/// `S::CallbackProtocol`, `S::BufferShape`, and so on.
///
/// The marker itself is required to implement the same auto-derivable
/// traits the IR types use, so generic IR types can carry
/// `#[derive(Clone, Debug, Eq, Hash, PartialEq)]` without manual
/// `impl` blocks.
pub trait Surface:
    'static + Sized + Clone + Copy + Debug + Default + Eq + Hash + PartialEq
{
    /// Size of a pointer on this surface.
    const POINTER_SIZE: ByteSize;

    /// Extracts the binding contract for this surface from a
    /// surface-tagged serialized payload.
    ///
    /// Returns `None` when the payload was produced for a different
    /// surface, so a consumer selecting the contract for one target never
    /// silently reads bindings classified for another.
    fn from_serialized(serialized: SerializedBindings) -> Option<Bindings<Self>>;

    /// Protocol foreign code uses to install and dispatch a callback
    /// trait. Native targets bind a vtable struct; wasm targets bind a
    /// set of imported functions.
    type CallbackProtocol: Clone
        + Debug
        + Eq
        + Hash
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + CallbackProtocolIntrospect<Self>;
    /// Native-slot layout chosen for an encoded buffer crossing.
    type BufferShape: Copy
        + Clone
        + Debug
        + Eq
        + Hash
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + BufferShapeRules;
    /// Carrier used to move an opaque handle across the boundary.
    type HandleCarrier: Copy
        + Clone
        + Debug
        + Eq
        + Hash
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>;
    /// Wire shape used when a foreign-provided closure registers with
    /// Rust.
    type IncomingClosureRegistration: Clone
        + Debug
        + Eq
        + Hash
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + ClosureRegistrationIntrospect;
    /// Wire shape used when a Rust-provided closure registers with
    /// foreign code.
    type OutgoingClosureRegistration: Clone
        + Debug
        + Eq
        + Hash
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + ClosureRegistrationIntrospect;
    /// Protocol used to drive an asynchronous callable to completion.
    type AsyncProtocol: Clone
        + Debug
        + Eq
        + Hash
        + PartialEq
        + Serialize
        + for<'de> Deserialize<'de>
        + AsyncProtocolIntrospect;
}

/// Introspection a callback protocol exposes for cross-cutting walks.
///
/// `Bindings<S>::validate` uses this to walk every callable inside a
/// callback declaration, regardless of how the surface lays out its
/// dispatch surface, and to collect every native symbol the protocol
/// references so the symbol-table membership invariant can be checked.
///
/// Callback methods are [`ImportedCallable<S>`] because foreign code
/// implements them and Rust calls in. The contained callables flow
/// params [`OutOfRust`](crate::OutOfRust) (Rust pushes args to the
/// foreign implementation) and returns [`IntoRust`](crate::IntoRust)
/// (foreign returns back to Rust).
pub trait CallbackProtocolIntrospect<S: Surface> {
    /// Iterates over the call shape of every method the protocol
    /// exposes.
    fn method_callables(&self) -> Box<dyn Iterator<Item = &ImportedCallable<S>> + '_>;
    /// Iterates over every native symbol the protocol references.
    fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_>;
}

/// Introspection an async protocol exposes for native-symbol collection.
pub trait AsyncProtocolIntrospect {
    /// Iterates over every native symbol the protocol references.
    fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_>;
}

/// Introspection a closure-registration shape exposes for symbol
/// collection.
pub trait ClosureRegistrationIntrospect {
    /// Iterates over every native symbol the registration references.
    fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_>;
}

/// The native call surface (host CPU, system linker, full C ABI).
///
/// Buffer descriptors cross by value or by pointer, handles cross as
/// integer carriers, callbacks dispatch through a registered vtable
/// struct, and async callables use the poll-handle protocol.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Native;

impl Surface for Native {
    const POINTER_SIZE: ByteSize = ByteSize::new(8);

    fn from_serialized(serialized: SerializedBindings) -> Option<Bindings<Self>> {
        match serialized {
            SerializedBindings::Native(bindings) => Some(bindings),
            SerializedBindings::Wasm32(_) => None,
        }
    }

    type CallbackProtocol = native::CallbackProtocol;
    type BufferShape = native::BufferShape;
    type HandleCarrier = native::HandleCarrier;
    type IncomingClosureRegistration = native::ClosureRegistration;
    type OutgoingClosureRegistration = native::ClosureRegistration;
    type AsyncProtocol = native::AsyncProtocol;
}

/// The 32-bit wasm call surface.
///
/// Buffers cross packed into a single integer or as a pointer-and-count
/// pair, handles cross as `u32`, callbacks dispatch through individually
/// imported functions, and async callables use the synchronous-poll
/// protocol the wasm runtime expects.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Wasm32;

impl Surface for Wasm32 {
    const POINTER_SIZE: ByteSize = ByteSize::new(4);

    fn from_serialized(serialized: SerializedBindings) -> Option<Bindings<Self>> {
        match serialized {
            SerializedBindings::Wasm32(bindings) => Some(bindings),
            SerializedBindings::Native(_) => None,
        }
    }

    type CallbackProtocol = wasm32::CallbackProtocol;
    type BufferShape = wasm32::BufferShape;
    type HandleCarrier = wasm32::HandleCarrier;
    type IncomingClosureRegistration = wasm32::IncomingClosureRegistration;
    type OutgoingClosureRegistration = wasm32::OutgoingClosureRegistration;
    type AsyncProtocol = wasm32::AsyncProtocol;
}

/// Concrete IR types for the [`Native`] surface.
pub mod native {
    use serde::{Deserialize, Serialize};

    use super::{
        AsyncProtocolIntrospect, CallbackProtocolIntrospect, ClosureRegistrationIntrospect, Native,
    };
    use crate::{ImportedCallable, ImportedMethodDecl, NativeSymbol, VTableSlot};

    /// How an encoded payload occupies native call slots.
    ///
    /// A slice is the borrowed `(pointer, count)` pair. A buffer
    /// descriptor is a struct with pointer, length, and capacity passed
    /// in one slot. A buffer pointer is a single slot holding the
    /// address of a descriptor that lives elsewhere.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum BufferShape {
        /// Pointer plus element count across two adjacent native slots.
        Slice,
        /// Buffer descriptor in a single native slot, by value.
        Buffer,
        /// Pointer to a buffer descriptor in a single native slot.
        BufferPointer,
    }

    /// Carrier that moves an opaque handle across the native boundary.
    ///
    /// `U64` and `USize` carry plain integer handles. `CallbackHandle`
    /// names the runtime struct that pairs a handle integer with a
    /// vtable pointer; callback-typed parameters cross as that struct
    /// rather than as a bare integer because the callee must dispatch
    /// through the paired vtable.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum HandleCarrier {
        /// 64-bit unsigned handle.
        U64,
        /// Pointer-width unsigned handle.
        USize,
        /// `boltffi::CallbackHandle` struct (handle plus vtable pointer).
        CallbackHandle,
    }

    /// Wire shape used when a closure registers across the native
    /// boundary.
    ///
    /// Closures cross as an invoke function pointer paired with a
    /// context pointer and a release function. The IR records the logical
    /// registration here; the
    /// position-specific layout is fixed by the wrapper variant that
    /// holds the closure payload:
    ///
    /// - At a parameter slot
    ///   ([`crate::IncomingParam::Closure`],
    ///   [`crate::OutgoingParam::Closure`]): two adjacent native
    ///   parameter slots — one function-pointer slot followed by one
    ///   pointer slot and one release-function slot. The C ABI of every
    ///   native target passes pointer-sized arguments uniformly.
    /// - At a return position
    ///   ([`crate::ReturnPlan::ClosureViaOutPointer`]): an explicit
    ///   out-pointer. The caller allocates storage for
    ///   `#[repr(C)] struct ClosureReturnStorage { invoke: fn pointer, context: *mut (), release: fn pointer }`
    ///   and passes its address as a trailing parameter; the callee
    ///   writes the pair through that pointer. The native return
    ///   slot stays free for an error status, so
    ///   `Result<closure, E>` composes without conflict. One ABI
    ///   across every native target — no register/sret variation,
    ///   no toolchain-handled struct-return guesswork.
    ///
    /// Both layouts describe the same logical pair, so one
    /// [`InvokeContextRelease`](Self::InvokeContextRelease) marker covers them.
    /// Distinct *wire* shapes are still distinct *variants* at the
    /// wrapper level — `ParamPlan`-side carriage versus
    /// `ClosureViaOutPointer` at return position.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum ClosureRegistration {
        /// Invoke function, context pointer, and release function.
        InvokeContextRelease,
    }

    impl ClosureRegistrationIntrospect for ClosureRegistration {
        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            Box::new(std::iter::empty())
        }
    }

    /// Protocol foreign code uses to install and dispatch a callback
    /// trait on the native surface.
    ///
    /// Foreign code allocates a vtable struct, fills its slots with
    /// function pointers, and calls `register` to install it. It then
    /// calls `create_handle` to bind one implementation to a handle.
    /// Rust dispatches through the [`CallbackVTable`] slots.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    pub struct CallbackProtocol {
        register: NativeSymbol,
        create_handle: NativeSymbol,
        vtable: CallbackVTable,
    }

    impl CallbackProtocol {
        pub(crate) fn new(
            register: NativeSymbol,
            create_handle: NativeSymbol,
            vtable: CallbackVTable,
        ) -> Self {
            Self {
                register,
                create_handle,
                vtable,
            }
        }

        /// Returns the native symbol that installs a vtable.
        pub fn register(&self) -> &NativeSymbol {
            &self.register
        }

        /// Returns the native symbol that creates a handle bound to an
        /// installed vtable.
        pub fn create_handle(&self) -> &NativeSymbol {
            &self.create_handle
        }

        /// Returns the vtable surface foreign code fills in.
        pub fn vtable(&self) -> &CallbackVTable {
            &self.vtable
        }
    }

    /// The set of vtable slots foreign code provides for a callback
    /// trait on the native surface.
    ///
    /// `free_slot` is invoked when Rust drops the foreign
    /// implementation. `clone_slot` is invoked when Rust duplicates the
    /// handle. Each method on the trait occupies its own slot named on
    /// the corresponding method declaration.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    pub struct CallbackVTable {
        free_slot: VTableSlot,
        clone_slot: VTableSlot,
        methods: Vec<ImportedMethodDecl<Native, VTableSlot>>,
    }

    impl CallbackVTable {
        pub(crate) fn new(
            free_slot: VTableSlot,
            clone_slot: VTableSlot,
            methods: Vec<ImportedMethodDecl<Native, VTableSlot>>,
        ) -> Self {
            Self {
                free_slot,
                clone_slot,
                methods,
            }
        }

        /// Returns the slot foreign code fills with the drop function.
        pub fn free_slot(&self) -> &VTableSlot {
            &self.free_slot
        }

        /// Returns the slot foreign code fills with the clone function.
        pub fn clone_slot(&self) -> &VTableSlot {
            &self.clone_slot
        }

        /// Returns the methods the foreign implementation must provide.
        pub fn methods(&self) -> &[ImportedMethodDecl<Native, VTableSlot>] {
            &self.methods
        }
    }

    /// Protocol used to drive an asynchronous callable to completion on
    /// the native surface.
    ///
    /// `NativeFuture` returns a runtime-native future-like value to the
    /// foreign side. `Continuation` runs to completion in Rust and
    /// invokes a callback symbol when finished. `PollHandle` returns a
    /// handle the foreign side polls until completion, then extracts
    /// the result and releases the handle.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum AsyncProtocol {
        /// Returns a runtime-native future-like value.
        NativeFuture,
        /// Reports completion by invoking a continuation symbol.
        Continuation {
            /// Native symbol used to deliver completion.
            symbol: NativeSymbol,
        },
        /// Returns a handle the foreign side polls.
        PollHandle {
            /// Carrier used for the async state handle.
            handle: HandleCarrier,
            /// Symbol that advances the operation without blocking.
            poll: NativeSymbol,
            /// Symbol that extracts the resolved value once ready.
            complete: NativeSymbol,
            /// Symbol that requests cancellation.
            cancel: NativeSymbol,
            /// Symbol that releases the async state.
            free: NativeSymbol,
            /// Symbol that retrieves the panic message after a failed
            /// operation.
            panic_message: NativeSymbol,
        },
        /// Foreign completes a callback method by invoking the
        /// completion function pointer passed in the vtable call.
        CallbackCompletion,
    }

    impl CallbackProtocolIntrospect<Native> for CallbackProtocol {
        fn method_callables(&self) -> Box<dyn Iterator<Item = &ImportedCallable<Native>> + '_> {
            Box::new(
                self.vtable()
                    .methods()
                    .iter()
                    .map(|method| method.callable()),
            )
        }

        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            Box::new([self.register(), self.create_handle()].into_iter())
        }
    }

    impl AsyncProtocolIntrospect for AsyncProtocol {
        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            match self {
                Self::NativeFuture | Self::CallbackCompletion => Box::new(std::iter::empty()),
                Self::Continuation { symbol } => Box::new(std::iter::once(symbol)),
                Self::PollHandle {
                    poll,
                    complete,
                    cancel,
                    free,
                    panic_message,
                    ..
                } => Box::new([poll, complete, cancel, free, panic_message].into_iter()),
            }
        }
    }
}

/// Concrete IR types for the [`Wasm32`] surface.
pub mod wasm32 {
    use serde::{Deserialize, Serialize};

    use super::{
        AsyncProtocolIntrospect, CallbackProtocolIntrospect, ClosureRegistrationIntrospect, Wasm32,
    };
    use crate::{ImportSymbol, ImportedCallable, ImportedMethodDecl, NativeSymbol};

    /// First handle value reserved for foreign-owned callback implementations.
    pub const FOREIGN_CALLBACK_HANDLE_START: u32 = 0x8000_0000;

    /// How an encoded payload occupies wasm call slots.
    ///
    /// A slice is the borrowed `(pointer, count)` pair, both as 32-bit
    /// integers in adjacent slots. A packed value folds the descriptor
    /// (pointer plus length) into one 64-bit integer; wasm extern
    /// signatures return at most one scalar, so packed is the only way
    /// a buffer leaves Rust through the return slot.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum BufferShape {
        /// Pointer plus element count across two adjacent `i32` slots.
        Slice,
        /// Buffer descriptor folded into a single `u64` slot.
        Packed,
    }

    /// Carrier that moves an opaque handle across the wasm boundary.
    ///
    /// Wasm32 functions exchange 32-bit integers; every handle is a
    /// `u32`.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum HandleCarrier {
        /// 32-bit unsigned handle.
        U32,
    }

    /// Wire shape used when a closure registers across the wasm
    /// boundary.
    ///
    /// Closures cross as a handle backed by imported invoke and release
    /// functions.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    pub struct IncomingClosureRegistration {
        call: ImportSymbol,
        free: ImportSymbol,
    }

    impl IncomingClosureRegistration {
        pub(crate) fn new(call: ImportSymbol, free: ImportSymbol) -> Self {
            Self { call, free }
        }

        /// Returns the import Rust calls to invoke the closure.
        pub fn call(&self) -> &ImportSymbol {
            &self.call
        }

        /// Returns the import Rust calls when the closure handle is released.
        pub fn free(&self) -> &ImportSymbol {
            &self.free
        }
    }

    impl ClosureRegistrationIntrospect for IncomingClosureRegistration {
        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            Box::new(std::iter::empty())
        }
    }

    /// Wire shape used when a Rust-provided closure registers across
    /// the wasm boundary.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    pub struct OutgoingClosureRegistration {
        call: NativeSymbol,
        free: NativeSymbol,
    }

    impl OutgoingClosureRegistration {
        pub(crate) fn new(call: NativeSymbol, free: NativeSymbol) -> Self {
            Self { call, free }
        }

        /// Returns the export foreign code calls to invoke the closure.
        pub fn call(&self) -> &NativeSymbol {
            &self.call
        }

        /// Returns the export foreign code calls when releasing the
        /// closure handle.
        pub fn free(&self) -> &NativeSymbol {
            &self.free
        }
    }

    impl ClosureRegistrationIntrospect for OutgoingClosureRegistration {
        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            Box::new([&self.call, &self.free].into_iter())
        }
    }

    /// Protocol foreign code uses to install and dispatch a callback
    /// trait on the wasm surface.
    ///
    /// Wasm has no vtable struct; instead, each dispatch role is its
    /// own imported function in the wasm module's import section. Rust
    /// links each import directly and calls it without an indirection.
    /// `create_handle` is the only Rust-exported entry point because
    /// wasm needs no separate vtable installation step.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    pub struct CallbackProtocol {
        create_handle: NativeSymbol,
        free: ImportSymbol,
        clone: ImportSymbol,
        methods: Vec<ImportedMethodDecl<Wasm32, ImportSymbol>>,
    }

    impl CallbackProtocol {
        pub(crate) fn new(
            create_handle: NativeSymbol,
            free: ImportSymbol,
            clone: ImportSymbol,
            methods: Vec<ImportedMethodDecl<Wasm32, ImportSymbol>>,
        ) -> Self {
            Self {
                create_handle,
                free,
                clone,
                methods,
            }
        }

        /// Returns the native symbol that creates a handle bound to a
        /// foreign implementation.
        pub fn create_handle(&self) -> &NativeSymbol {
            &self.create_handle
        }

        /// Returns the wasm import that drops the foreign implementation.
        pub fn free(&self) -> &ImportSymbol {
            &self.free
        }

        /// Returns the wasm import that duplicates the handle.
        pub fn clone_import(&self) -> &ImportSymbol {
            &self.clone
        }

        /// Returns the wasm imports the foreign implementation must
        /// provide for each method.
        pub fn methods(&self) -> &[ImportedMethodDecl<Wasm32, ImportSymbol>] {
            &self.methods
        }
    }

    /// Protocol used to drive an asynchronous callable to completion on
    /// the wasm surface.
    ///
    /// `PollHandle` returns a handle the foreign side polls; the
    /// synchronous variant of `poll` is required because wasm hosts
    /// drive the loop themselves and need a blocking step.
    #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
    #[non_exhaustive]
    pub enum AsyncProtocol {
        /// Returns a handle the foreign side polls.
        PollHandle {
            /// Carrier used for the async state handle.
            handle: HandleCarrier,
            /// Symbol that advances the operation while the foreign
            /// side blocks waiting for completion.
            poll_sync: NativeSymbol,
            /// Symbol that extracts the resolved value once ready.
            complete: NativeSymbol,
            /// Symbol that requests cancellation.
            cancel: NativeSymbol,
            /// Symbol that releases the async state.
            free: NativeSymbol,
            /// Symbol that retrieves the panic message after a failed
            /// operation.
            panic_message: NativeSymbol,
        },
        /// Foreign starts a callback method through an import and
        /// completes it by calling this Rust export.
        CallbackCompletion {
            /// Symbol foreign code calls to complete the pending
            /// callback request.
            complete: NativeSymbol,
        },
    }

    impl CallbackProtocolIntrospect<Wasm32> for CallbackProtocol {
        fn method_callables(&self) -> Box<dyn Iterator<Item = &ImportedCallable<Wasm32>> + '_> {
            Box::new(self.methods().iter().map(|method| method.callable()))
        }

        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            Box::new(std::iter::once(self.create_handle()))
        }
    }

    impl AsyncProtocolIntrospect for AsyncProtocol {
        fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
            match self {
                Self::PollHandle {
                    poll_sync,
                    complete,
                    cancel,
                    free,
                    panic_message,
                    ..
                } => Box::new([poll_sync, complete, cancel, free, panic_message].into_iter()),
                Self::CallbackCompletion { complete } => Box::new(std::iter::once(complete)),
            }
        }
    }
}

/// Validates that an encoded buffer shape is allowed on a parameter
/// (lower) crossing.
///
/// Defined as a free helper rather than a method so each surface's
/// rules live next to the surface module that owns them.
pub trait BufferShapeRules {
    /// Returns `true` when this shape may appear in a parameter
    /// crossing.
    fn is_valid_in_param(&self) -> bool;
    /// Returns `true` when this shape may appear in a return or error
    /// crossing.
    fn is_valid_in_return(&self) -> bool;
}

impl BufferShapeRules for native::BufferShape {
    fn is_valid_in_param(&self) -> bool {
        true
    }

    fn is_valid_in_return(&self) -> bool {
        !matches!(self, Self::Slice)
    }
}

impl BufferShapeRules for wasm32::BufferShape {
    fn is_valid_in_param(&self) -> bool {
        !matches!(self, Self::Packed)
    }

    fn is_valid_in_return(&self) -> bool {
        !matches!(self, Self::Slice)
    }
}
