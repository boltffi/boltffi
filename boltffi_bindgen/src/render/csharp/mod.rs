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
//! - `ast`: pure C# AST. Self-contained nodes whose Display produces
//!   standalone C# source. Knows the IR (lifts from it) but nothing
//!   downstream.
//! - `plan`: FFI-shaped view model built on `ast` payloads. Models
//!   records, enums, functions, methods, params: what crosses the ABI.
//! - `lower`: decision layer. Walks the IR and produces a plan.
//! - `emit`: orchestrator plus ABI-op → C# syntax helpers.
//!
//! Supporting modules:
//!
//! - `names`: legacy snake_case → PascalCase / camelCase helpers.
//!   Being absorbed into `ast::identifier`; will go away once every
//!   call site has migrated.
//! - `templates`: Askama bindings over `plan`, rendered by `emit`.
//!   Snapshot tests live alongside.
//!
//! Module dependencies: `ast` builds on the IR. `plan` builds on
//! `ast`. `templates`, `lower`, and `emit` all build on `plan` and
//! `ast`. `lower` and `emit` cooperate: `lower` calls `emit`'s syntax
//! helpers to pre-render wire expressions into the plan; `emit`'s
//! orchestrator calls `lower` to produce that plan.

mod ast;
mod emit;
mod lower;
mod names;
mod plan;
mod templates;

pub use ast::{CSharpClassName, CSharpNamespace};
pub use emit::{CSharpEmitter, CSharpFile, CSharpOutput};
pub use names::NamingConvention;

use boltffi_ffi_rules::naming::{LibraryName, Name};

#[derive(Debug, Clone, Default)]
pub struct CSharpOptions {
    /// Override the native library name used in `[DllImport("...")]` declarations.
    /// Defaults to the crate/package name when `None`.
    pub library_name: Option<Name<LibraryName>>,
}
