use std::collections::BTreeSet;

use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, CanonicalName, ClassDecl, ClassId, ConstantDecl,
    ConstantId, ConstantOwner, CustomTypeDecl, CustomTypeId, DataEnumDecl, DeclarationId,
    DeclarationRef, EnumDecl, EnumId, FunctionDecl, FunctionId, RecordDecl, RecordId, StreamDecl,
    StreamId, Surface,
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
    transparent_enum_ids: BTreeSet<EnumId>,
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
            transparent_enum_ids: bindings
                .decls()
                .iter()
                .filter_map(|declaration| match DeclarationRef::from(declaration) {
                    DeclarationRef::Enum(EnumDecl::Data(enumeration))
                        if enumeration.has_transparent_variants()
                            && !enumeration.is_error_payload() =>
                    {
                        Some(enumeration.id())
                    }
                    _ => None,
                })
                .collect(),
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

    pub(crate) fn without_transparent_enums(mut self, excluded: BTreeSet<EnumId>) -> Self {
        self.transparent_enum_ids
            .retain(|id| !excluded.contains(id));
        self
    }

    pub(crate) fn transparent_enum_ids(&self) -> &BTreeSet<EnumId> {
        &self.transparent_enum_ids
    }

    pub(crate) fn enum_is_transparent(&self, id: EnumId) -> bool {
        self.transparent_enum_ids.contains(&id)
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

    /// Iterates over constants associated with the given exported type.
    pub fn associated_constants(
        &self,
        owner: ConstantOwner,
    ) -> impl Iterator<Item = &'bindings ConstantDecl<S>> + '_ {
        self.bindings.decls().iter().filter_map(move |declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Constant(constant) if constant.owner() == Some(owner) => {
                    Some(constant)
                }
                _ => None,
            }
        })
    }

    /// Returns the source name of an associated constant owner.
    pub fn constant_owner_name(&self, owner: ConstantOwner) -> Option<&'bindings CanonicalName> {
        match owner {
            ConstantOwner::Record(id) => self.record(id).map(RecordDecl::name),
            ConstantOwner::Enum(id) => self.enumeration(id).map(EnumDecl::name),
            ConstantOwner::Class(id) => self.class(id).map(ClassDecl::name),
        }
    }

    /// Returns the names of the data enums this render emits whose
    /// transparent variants carry the record as their payload, in contract
    /// order.
    ///
    /// Every backend that gives a payload record a supertype reads the list
    /// from here, so the record's declaration and the enum's own cannot
    /// disagree about which conformances exist. Enums the render prunes are
    /// left out: their type never reaches the generated source, so a record
    /// conforming to one would name a type nothing declares.
    pub fn transparent_conformances(
        &self,
        id: RecordId,
    ) -> impl Iterator<Item = &'bindings CanonicalName> + '_ {
        self.transparent_enums(id).map(DataEnumDecl::name)
    }

    pub(crate) fn transparent_enums(
        &self,
        id: RecordId,
    ) -> impl Iterator<Item = &'bindings DataEnumDecl<S>> + '_ {
        self.bindings
            .decls()
            .iter()
            .filter_map(|declaration| {
                DeclarationRef::enumeration(DeclarationRef::from(declaration))
            })
            .filter_map(move |enumeration| match enumeration {
                EnumDecl::Data(enumeration)
                    if self.enum_is_transparent(enumeration.id())
                        && enumeration
                            .variants()
                            .iter()
                            .any(|variant| variant.transparent_payload() == Some(id)) =>
                {
                    Some(enumeration.as_ref())
                }
                _ => None,
            })
    }

    /// Returns whether the record is the payload of a transparent data-enum
    /// variant this render emits, i.e. whether its rendered type declares
    /// supertypes.
    pub fn is_transparent_payload(&self, id: RecordId) -> bool {
        self.transparent_enums(id).next().is_some()
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
