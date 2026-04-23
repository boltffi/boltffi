//! C# backend. Generates `.cs` source files that call into the C ABI
//! exported by BoltFFI, using P/Invoke (`[DllImport]`) for the boundary
//! crossing.
//!
//! # Module layout
//!
//! The backend transforms the language-agnostic IR into `.cs` files:
//!
//! ```text
//! FfiContract + AbiContract
//!         │
//!         ▼  lower: walk the IR, decide supported + blittable paths
//! CSharpModule (plan: data shapes the templates consume)
//!         │
//!         ▼  emit: orchestrate + render templates
//! Vec<CSharpFile>
//! ```
//!
//! Core modules:
//!
//! - `plan`: view model. `CSharpType` is the central vocabulary; all
//!   wire expressions are pre-rendered strings so templates stay dumb.
//! - `lower`: decision layer. Produces the plan from the IR.
//! - `emit`: orchestrator plus ABI-op → C# syntax helpers.
//!
//! Supporting modules:
//!
//! - `names`: how elements get named in C#. Used by `plan`, `lower`,
//!   and `emit`.
//! - `templates`: Askama bindings over `plan`, rendered by `emit`.
//!   Snapshot tests live alongside.
//!
//! Module dependencies: `names` is a leaf. `plan` builds on `names`
//! plus the IR. `templates`, `lower`, and `emit` all build on `plan`.
//! `lower` and `emit` cooperate: `lower` calls `emit`'s syntax
//! helpers to pre-render wire expressions into the plan; `emit`'s
//! orchestrator calls `lower` to produce that plan.

mod emit;
mod lower;
mod names;
mod plan;
mod templates;

pub use emit::{CSharpEmitter, CSharpOutput};
pub use names::NamingConvention;
pub use plan::*;

use boltffi_ffi_rules::naming::{LibraryName, Name};

#[derive(Debug, Clone, Default)]
pub struct CSharpOptions {
    /// Override the native library name used in `[DllImport("...")]` declarations.
    /// Defaults to the crate/package name when `None`.
    pub library_name: Option<Name<LibraryName>>,
}
