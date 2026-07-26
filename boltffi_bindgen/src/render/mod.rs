//! Legacy backends that generate target-language source files from an [`AbiContract`].
//!
//! Each backend is split into three parts:
//!
//! - A **lowerer** that walks the [`AbiContract`] and maps each `AbiCall`,
//!   `AbiRecord`, `AbiEnum`, and `AbiStream` into language-specific plan
//!   structs. These plan structs carry everything a template needs to render:
//!   type names, method signatures, wire read/write expressions, native
//!   function declarations.
//!
//! - An **emitter** that feeds those plan structs into Askama templates and
//!   concatenates the output into a single source file.
//!
//! - A set of **Askama templates** (`.txt` files under `templates/`) that
//!   contain the actual target-language syntax with template placeholders.
//!
//! All backends implement the [`Renderer`] trait.

pub mod c;
pub mod dart;
pub mod jni;
pub mod kmp;
pub mod kotlin;

use std::collections::HashMap;

use crate::ir::{AbiContract, FfiContract};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeConversion {
    UuidString,
    UrlString,
}

#[derive(Debug, Clone)]
pub struct TypeMapping {
    pub native_type: String,
    pub conversion: TypeConversion,
}

pub type TypeMappings = HashMap<String, TypeMapping>;

/// Shared interface for all target-language backends.
///
/// Receives both the semantic [`FfiContract`] for type definitions and naming,
/// and the resolved [`AbiContract`] for wire ops and parameter strategies.
pub trait Renderer {
    type Output;

    /// Walks the [`FfiContract`] and [`AbiContract`] and generates the
    /// complete source output for this backend.
    ///
    /// The [`FfiContract`] provides type definitions, naming, and API
    /// structure. The [`AbiContract`] provides the resolved wire ops,
    /// parameter strategies, and async machinery that the lowerer has
    /// already computed.
    fn render(contract: &FfiContract, abi: &AbiContract) -> Self::Output;
}
