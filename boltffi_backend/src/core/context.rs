use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, ClassDecl, ClassId, ConstantDecl, ConstantId,
    CustomTypeDecl, CustomTypeId, DeclarationId, DeclarationRef, EnumDecl, EnumId, FunctionDecl,
    FunctionId, RecordDecl, RecordId, StreamDecl, StreamId, Surface,
};

use crate::core::capabilities::BindingCapabilityAnalysis;
use crate::core::{
    BindingCapability, CapabilityRequirements, CoverageMode, CustomTypeMapping,
    ResolvedCustomTypeMappings,
};

/// Read-only state shared while one target renders a binding contract.
#[non_exhaustive]
pub struct RenderContext<'bindings, S: Surface> {
    bindings: &'bindings Bindings<S>,
    target: &'static str,
    custom_type_mappings: ResolvedCustomTypeMappings,
    capability_analysis: Option<BindingCapabilityAnalysis>,
    coverage_mode: CoverageMode,
}

impl<'bindings, S: Surface> RenderContext<'bindings, S> {
    /// Creates a render context for a target.
    pub fn new(
        bindings: &'bindings Bindings<S>,
        target: &'static str,
        coverage_mode: CoverageMode,
    ) -> Self {
        Self {
            bindings,
            target,
            custom_type_mappings: ResolvedCustomTypeMappings::default(),
            capability_analysis: None,
            coverage_mode,
        }
    }

    /// Adds resolved custom type mappings.
    pub fn with_custom_type_mappings(mut self, mappings: ResolvedCustomTypeMappings) -> Self {
        self.custom_type_mappings = mappings;
        self
    }

    /// Adds contract-scoped capability analysis for this render.
    pub(crate) fn with_capability_analysis(mut self, analysis: BindingCapabilityAnalysis) -> Self {
        self.capability_analysis = Some(analysis);
        self
    }

    /// Returns the binding contract being rendered.
    pub const fn bindings(&self) -> &'bindings Bindings<S> {
        self.bindings
    }

    /// Returns the backend target name.
    pub const fn target(&self) -> &'static str {
        self.target
    }

    /// Returns the coverage policy for this render.
    pub const fn coverage_mode(&self) -> CoverageMode {
        self.coverage_mode
    }

    /// Returns requirements precomputed for one declaration in this contract.
    pub(crate) fn capability_requirements(
        &self,
        declaration: DeclarationId,
    ) -> Option<&CapabilityRequirements<BindingCapability>> {
        self.capability_analysis
            .as_ref()
            .and_then(|analysis| analysis.declaration_requirements(declaration))
    }

    /// Returns the record declaration with the given id.
    pub fn record(&self, id: RecordId) -> Option<&'bindings RecordDecl<S>> {
        self.find(DeclarationId::Record(id), DeclarationRef::record)
    }

    /// Returns the enum declaration with the given id.
    pub fn enumeration(&self, id: EnumId) -> Option<&'bindings EnumDecl<S>> {
        self.find(DeclarationId::Enum(id), DeclarationRef::enumeration)
    }

    /// Returns the class declaration with the given id.
    pub fn class(&self, id: ClassId) -> Option<&'bindings ClassDecl<S>> {
        self.find(DeclarationId::Class(id), DeclarationRef::class)
    }

    /// Returns the callback declaration with the given id.
    pub fn callback(&self, id: CallbackId) -> Option<&'bindings CallbackDecl<S>> {
        self.find(DeclarationId::Callback(id), DeclarationRef::callback)
    }

    /// Returns the stream declaration with the given id.
    pub fn stream(&self, id: StreamId) -> Option<&'bindings StreamDecl<S>> {
        self.find(DeclarationId::Stream(id), DeclarationRef::stream)
    }

    /// Returns the constant declaration with the given id.
    pub fn constant(&self, id: ConstantId) -> Option<&'bindings ConstantDecl<S>> {
        self.find(DeclarationId::Constant(id), DeclarationRef::constant)
    }

    /// Returns the function declaration with the given id.
    pub fn function(&self, id: FunctionId) -> Option<&'bindings FunctionDecl<S>> {
        self.find(DeclarationId::Function(id), DeclarationRef::function)
    }

    /// Returns the custom type declaration with the given id.
    pub fn custom_type(&self, id: CustomTypeId) -> Option<&'bindings CustomTypeDecl> {
        self.find(DeclarationId::CustomType(id), DeclarationRef::custom_type)
    }

    /// Returns the target mapping for the custom type id.
    pub fn custom_type_mapping(&self, id: CustomTypeId) -> Option<&CustomTypeMapping> {
        self.custom_type_mappings.get(id)
    }

    fn find<T>(
        &self,
        id: DeclarationId,
        select: impl Fn(DeclarationRef<'bindings, S>) -> Option<&'bindings T>,
    ) -> Option<&'bindings T> {
        self.bindings.decls().iter().find_map(|declaration| {
            (declaration.id() == id)
                .then(|| select(DeclarationRef::from(declaration)))
                .flatten()
        })
    }
}
