//! Swift target rendered through the C ABI bridge.

mod c_abi;
mod codec;
mod default_value;
mod name_style;
mod primitive;
mod render;
mod syntax;

use std::path::PathBuf;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration,
        ResolvedCustomTypeMappings, Result, Target, contract::sealed, host,
    },
};

pub use crate::core::{
    CustomTypeConversion as SwiftCustomConversion, CustomTypeMapping as SwiftCustomMapping,
};
use name_style::Name;
pub use name_style::{SwiftFile, SwiftModule};
use syntax::{ArgumentList, Expression, Syntax, TypeName};

/// Swift host renderer for a generated module.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct SwiftHost {
    module: SwiftModule,
    file: SwiftFile,
    c_header: PathBuf,
    custom_mappings: crate::core::CustomTypeMappingSet,
}

impl SwiftHost {
    const TARGET: &'static str = "swift";

    /// Creates a Swift host renderer.
    pub fn new(module: impl Into<String>) -> Result<Self> {
        let module = SwiftModule::parse(module)?;
        Ok(Self {
            file: SwiftFile::from_module(&module),
            module,
            c_header: PathBuf::from("boltffi.h"),
            custom_mappings: crate::core::CustomTypeMappingSet::default(),
        })
    }

    /// Selects the generated Swift source file.
    pub fn file(mut self, file: impl Into<String>) -> Result<Self> {
        self.file = SwiftFile::parse(file)?;
        Ok(self)
    }

    /// Selects the generated C header path.
    pub fn c_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.c_header = path.into();
        self
    }

    /// Registers a Swift API mapping for one custom type.
    pub fn custom_mapping(
        mut self,
        custom_type: impl Into<String>,
        mapping: SwiftCustomMapping,
    ) -> Self {
        self.custom_mappings.insert(custom_type, mapping);
        self
    }

    /// Creates the backend target stack for this Swift host.
    pub fn into_target(self) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(
            self.clone(),
            CBridge::new(self.c_header.clone())?,
        ))
    }

    /// Returns the Swift module name.
    pub fn module(&self) -> &SwiftModule {
        &self.module
    }

    /// Returns the generated Swift file name.
    pub fn file_name(&self) -> &SwiftFile {
        &self.file
    }

    fn custom_type_name(mapping: &SwiftCustomMapping) -> TypeName {
        TypeName::new(mapping.target_type().as_str())
    }

    fn custom_type_decode(
        mapping: &SwiftCustomMapping,
        representation: Expression,
    ) -> Result<Expression> {
        Ok(match mapping.conversion() {
            SwiftCustomConversion::UuidString => Expression::forced(Expression::call(
                "UUID",
                [Expression::labeled("uuidString", representation)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )),
            SwiftCustomConversion::UrlString => Expression::forced(Expression::call(
                "URL",
                [Expression::labeled("string", representation)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )),
        })
    }

    fn custom_type_encode(mapping: &SwiftCustomMapping, value: Expression) -> Expression {
        match mapping.conversion() {
            SwiftCustomConversion::UuidString => Expression::member(value, "uuidString"),
            SwiftCustomConversion::UrlString => Expression::member(value, "absoluteString"),
        }
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: Self::TARGET,
            shape,
        }
    }
}

impl host::HostBackend for SwiftHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        Self::TARGET
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
            .stable(BindingCapability::Callbacks)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn custom_type_mappings(
        &self,
        bindings: &Bindings<Self::Surface>,
    ) -> Result<ResolvedCustomTypeMappings> {
        self.custom_mappings
            .resolve(bindings, Self::TARGET, |declaration| {
                Name::new(declaration.name()).type_name().to_string()
            })
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(decl, bridge, context)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration(decl, bridge, context)?.render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, bridge, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Class::from_declaration(decl, bridge, context)?.render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Callback::from_declaration(decl, bridge, context)?.render()
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Stream::from_declaration(decl, bridge, context)?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Constant::from_declaration(decl, bridge, context)?.render()
    }

    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::CustomType::from_declaration(decl, context)?.render()
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        render::Module::new(self, declarations).render()
    }
}

impl sealed::HostBackend for SwiftHost {}
