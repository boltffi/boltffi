//! Backend composition primitives.

pub mod bridge;
/// Capability declarations and checks.
pub mod capabilities;
/// Shared render context.
pub mod context;
/// Bridge contract trait.
pub mod contract;
/// Backend coverage reports.
pub mod coverage;
/// Custom types mapping contracts.
pub mod custom_types_mapping;
/// Backend errors.
pub mod error;
/// Generated file containers.
pub mod files;
/// Host backend traits.
pub mod host;
pub(crate) mod name_case;
/// Typed language syntax fragments.
pub mod syntax;
/// Target composition.
pub mod target;

pub use bridge::{BridgeBackend, BridgeOutput, BridgeStack};
pub use capabilities::{
    BindingCapability, BridgeCapabilities, BridgeCapability, CapabilityRequirements, CapabilitySet,
    CapabilityStatus, HostCapabilities,
};
pub use context::RenderContext;
pub use contract::BridgeContract;
pub use coverage::{CoverageMode, CoverageReport, DeclarationLabel, UnsupportedDeclaration};
pub use custom_types_mapping::{
    CustomTypeConversion, CustomTypeMapping, CustomTypeMappingSet, ResolvedCustomTypeMappings,
    TargetTypeName,
};
pub use error::{BackendError, Error, Result};
pub use files::{
    AllDeclarations, AuxChunk, Diagnostic, Emitted, FallbackPolicy, FileAssembler, FileGroup,
    FileLayout, FilePath, FilePlan, GeneratedFile, GeneratedOutput, HelperId, HelperPolicy,
    ImportDirective, RenderedDeclaration, TextChunk,
};
pub use host::HostBackend;
pub use syntax::{LanguageSyntax, SyntaxFragment};
pub use target::{BridgeLayer, Target};
