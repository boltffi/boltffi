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
//! CSharpModulePlan (plan: data shapes the templates consume)
//!         │
//!         ▼  emit: orchestrate + render templates
//! Vec<CSharpFile>
//! ```
//!
//! Core modules:
//!
//! - `ast`: pure C# AST. Self-contained nodes whose Display produces
//!   standalone C# source. Knows the IR (lifts from it) but nothing
//!   downstream.
//! - `plan`: FFI-shaped view model built on `ast` payloads. Models
//!   records, enums, functions, methods, params: what crosses the ABI.
//! - `lower`: decision layer. Walks the IR and produces a plan,
//!   including the typed AST sub-trees for the size, encode, and
//!   decode wire phases.
//! - `emit`: orchestrator. Drives the lowerer and renders each plan
//!   entry through its Askama template.
//!
//! Supporting module:
//!
//! - `templates`: Askama bindings over `plan`, rendered by `emit`.
//!   Snapshot tests live alongside.
//!
//! Module dependencies: `ast` builds on the IR. `plan` builds on
//! `ast`. `templates`, `lower`, and `emit` all build on `plan` and
//! `ast`. `emit` calls `lower` to produce the plan; everything else
//! flows downstream from there.

mod ast;
mod emit;
mod lower;
mod plan;
mod templates;

pub use emit::CSharpEmitter;

use boltffi_ffi_rules::naming::{LibraryName, Name};

#[derive(Debug, Clone, Default)]
pub struct CSharpOptions {
    /// Override the native library name used in `[DllImport("...")]` declarations.
    /// Defaults to the crate/package name when `None`.
    pub library_name: Option<Name<LibraryName>>,
}
