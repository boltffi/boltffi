//! Backend rendering contracts for classified BoltFFI bindings.
//!
//! `boltffi_backend` is the layer below `boltffi_bindgen` and above
//! generated target-language files. It accepts validated
//! [`boltffi_binding::Bindings`] values and exposes typed composition for
//! bridge layers and host-language renderers.
//!
//! Bridge renderers produce the ABI surface a host consumes. Host
//! renderers produce the target-language API over that bridge. A
//! [`Target`] ties both together with an associated-type constraint, so a
//! host cannot be paired with a bridge stack whose contract it does not
//! accept.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bridge;
pub mod core;
pub mod target;

pub use core::{
    AllDeclarations, AuxChunk, BackendError, BindingCapability, BridgeCapabilities,
    BridgeCapability, CapabilityRequirements, CapabilitySet, CapabilityStatus, CoverageMode,
    CoverageReport, CustomTypeConversion, CustomTypeMapping, CustomTypeMappingSet,
    DeclarationLabel, Diagnostic, Emitted, Error, FallbackPolicy, FileAssembler, FileGroup,
    FileLayout, FilePath, FilePlan, GeneratedFile, GeneratedOutput, HelperId, HelperPolicy,
    HostCapabilities, ImportDirective, LanguageSyntax, RenderContext, RenderedDeclaration,
    ResolvedCustomTypeMappings, Result, SyntaxFragment, Target, TargetTypeName, TextChunk,
    UnsupportedDeclaration,
};
