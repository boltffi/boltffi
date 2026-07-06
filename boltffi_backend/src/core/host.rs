//! Host-language renderers.
//!
//! A host renderer emits target-language source over one bridge
//! contract. Its associated [`HostBackend::Bridge`] type is the only
//! bridge shape it can receive, and [`crate::Target`] uses that
//! associated type to reject invalid host/bridge pairings at compile
//! time.

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Surface,
};

use crate::core::{
    BridgeCapability, BridgeContract, CapabilityRequirements, CoverageReport, Emitted,
    GeneratedOutput, HostCapabilities, LanguageSyntax, RenderContext, RenderedDeclaration,
    ResolvedCustomTypeMappings, Result, contract::sealed,
};

/// Host renderer for one target language.
#[allow(private_bounds)]
pub trait HostBackend: sealed::HostBackend {
    /// Binding surface this host renders.
    type Surface: Surface;
    /// Bridge contract this host accepts.
    type Bridge: BridgeContract<Surface = Self::Surface>;
    /// Language syntax fragments this host emits.
    type Syntax: LanguageSyntax;

    /// Returns the target name used in diagnostics.
    fn name(&self) -> &'static str;

    /// Returns binding capabilities this host can render.
    fn binding_capabilities(&self) -> HostCapabilities;

    /// Returns bridge capabilities this host requires.
    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability>;

    /// Resolves configured custom type mappings for this binding contract.
    fn custom_type_mappings(
        &self,
        _bindings: &Bindings<Self::Surface>,
    ) -> Result<ResolvedCustomTypeMappings> {
        Ok(ResolvedCustomTypeMappings::default())
    }

    /// Computes target-owned coverage before per-declaration rendering.
    fn preflight_coverage(
        &self,
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<CoverageReport> {
        Ok(CoverageReport::new())
    }

    /// Renders a record declaration.
    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders an enum declaration.
    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a free function declaration.
    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a class declaration.
    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a callback trait declaration.
    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a stream declaration.
    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a constant declaration.
    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a custom type declaration.
    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Assembles collected declaration fragments into generated host files.
    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput>;
}
