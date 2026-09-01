//! The binding contract for an FFI-exported Rust API.
//!
//! BoltFFI turns a Rust crate annotated with `#[data]`, `#[export]`, and
//! `#[data(impl)]` into target-language source. The path from one to the
//! other runs through several crates; this one owns the middle stage.
//!
//! # The pipeline
//!
//! ```text
//!   user crate (Rust)
//!         │
//!         │  boltffi_macros scan the source
//!         ▼
//!   boltffi_ast::SourceContract           ← what the user wrote
//!         │
//!         │  lower (this crate)
//!         ▼
//!   Bindings                              ← what crosses, in what shape
//!         │
//!         │  macros and backends consume the binding facts
//!         ▼
//!   Rust glue + serialized metadata in the user's .rlib
//!         │
//!         │  boltffi_bindgen reads the metadata back into Bindings
//!         ▼
//!   per-language source                   ← Swift, Kotlin, Python, C, …
//! ```
//!
//! `boltffi_ast` records what the user wrote. This crate decides what
//! that source means at the FFI boundary: a record is direct or encoded,
//! an enum is C-style or data-bearing, a callable gets concrete lower
//! and lift plans, a native symbol is picked and validated. The
//! lowering pass runs once; nothing downstream re-runs it on the same
//! source. Any lane that must classify outside the pass applies the
//! same exported rule rather than keeping a copy.
//!
//! # Public surfaces
//!
//! Three public entry points sit alongside [`ir`]:
//!
//! - [`lower`] is the macro-facing API. Given a
//!   [`boltffi_ast::SourceContract`] and a target [`SurfaceLower`]
//!   (today [`Native`] or [`Wasm32`]) it produces a [`Bindings<S>`].
//!   `boltffi_macros` invokes [`lower`] while expanding the user's
//!   crate.
//! - [`ir`] is the data-only surface every downstream consumer
//!   imports. `boltffi_bindgen` reconstructs a [`Bindings<S>`] from
//!   the serialized metadata embedded in the user's compiled
//!   artifact and reads it through the [`ir`] types.
//! - [`direct_record_fields`] is the record classification rule on its
//!   own: effective `repr(C)`, at least one field, every field an
//!   admissible fixed-width primitive, and byte layout agreement across
//!   the supported native ABI alignment profiles. It is the single
//!   statement of the rule `is_direct` applies during lowering, exported
//!   so a lane that cannot run the full pass still classifies a record
//!   identically to the compiled library.
//!
//! # What this crate does not do
//!
//! No target-language code generation. No filesystem writes. No
//! dependency on any specific backend. The crate ends with a [`Bindings`]
//! value; turning that into Swift, Kotlin, Python, or any other target
//! lives in separate backend crates.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod ir;
mod lower;

pub use ir::*;
pub use ir::{ErrorChannel, ErrorPlacement};
pub use lower::{
    DeclarationFamily, DeclarationMap, LowerError, LowerErrorKind, LoweredBindings, SurfaceLower,
    UnsupportedType, direct_record_fields, lower, lower_with_declarations,
};
