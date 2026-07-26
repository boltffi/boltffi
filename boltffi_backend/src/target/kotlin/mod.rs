//! Kotlin target rendered through the JNI bridge.

mod codec;
mod name_style;
mod primitive;
mod render;
mod syntax;
mod tuple;

use std::path::PathBuf;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::{
        c::CBridge,
        jni::{JniBridge, JniBridgeContract},
    },
    core::{
        BindingCapability, BridgeCapability, BridgeLayer, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration,
        ResolvedCustomTypeMappings, Result, Target, contract::sealed, host,
    },
    target::jvm::{LibraryName, NativeLibraries},
};

pub use crate::core::{
    CustomTypeConversion as KotlinCustomConversion, CustomTypeMapping as KotlinCustomMapping,
};
pub use crate::target::jvm::{DesktopLoader as KotlinDesktopLoader, LibraryName as KotlinLibrary};
use name_style::Name;
pub use name_style::{KotlinFile, KotlinPackage};
use syntax::{ArgumentList, Expression, Identifier, Syntax, TypeName};

/// Public API layout for generated Kotlin declarations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum KotlinApiStyle {
    /// Render declarations directly in the Kotlin package.
    #[default]
    TopLevel,
    /// Render declarations inside the configured module object.
    ModuleObject,
}

/// Factory layout for generated Kotlin class initializers.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum KotlinFactoryStyle {
    /// Render initializer overloads as Kotlin constructors when signatures allow it.
    #[default]
    Constructors,
    /// Render initializers only as companion object methods.
    CompanionMethods,
}

/// Kotlin host renderer for a generated JNI owner class.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KotlinHost {
    package: KotlinPackage,
    file: KotlinFile,
    c_header: PathBuf,
    jni_source: PathBuf,
    native_libraries: NativeLibraries,
    api_style: KotlinApiStyle,
    factory_style: KotlinFactoryStyle,
    custom_mappings: crate::core::CustomTypeMappingSet,
}

impl KotlinHost {
    const TARGET: &'static str = "kotlin";

    /// Creates a Kotlin host renderer.
    pub fn new(package: impl Into<String>, file: impl Into<String>) -> Result<Self> {
        Ok(Self {
            package: KotlinPackage::parse(package)?,
            file: KotlinFile::parse(file)?,
            c_header: PathBuf::from("jni/boltffi.h"),
            jni_source: PathBuf::from("jni/jni_glue.c"),
            native_libraries: NativeLibraries::boltffi()?,
            api_style: KotlinApiStyle::default(),
            factory_style: KotlinFactoryStyle::default(),
            custom_mappings: crate::core::CustomTypeMappingSet::default(),
        })
    }

    /// Selects the generated C header path.
    pub fn c_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.c_header = path.into();
        self
    }

    /// Selects the generated JNI source path.
    pub fn jni_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.jni_source = path.into();
        self
    }

    /// Selects the Android native library load name.
    pub fn android_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries = self
            .native_libraries
            .with_android(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop JNI wrapper library load name.
    pub fn desktop_jni_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries = self
            .native_libraries
            .with_desktop_jni(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop fallback library load name.
    pub fn desktop_fallback_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries = self
            .native_libraries
            .with_desktop_fallback(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop native-library loading policy.
    pub fn desktop_loader(mut self, loader: KotlinDesktopLoader) -> Self {
        self.native_libraries = self.native_libraries.with_desktop_loader(loader);
        self
    }

    /// Selects the generated Kotlin API layout.
    pub fn api_style(mut self, style: KotlinApiStyle) -> Self {
        self.api_style = style;
        self
    }

    /// Selects the generated Kotlin class factory layout.
    pub fn factory_style(mut self, style: KotlinFactoryStyle) -> Self {
        self.factory_style = style;
        self
    }

    /// Registers a Kotlin API mapping for one custom type id.
    pub fn custom_mapping(
        mut self,
        custom_type: impl Into<String>,
        mapping: KotlinCustomMapping,
    ) -> Self {
        self.custom_mappings.insert(custom_type, mapping);
        self
    }

    /// Creates the backend target stack for this Kotlin host.
    pub fn into_target(self) -> Result<Target<Self, BridgeLayer<CBridge, JniBridge>>> {
        Ok(Target::new(
            self.clone(),
            BridgeLayer::new(
                CBridge::new(self.c_header.clone())?,
                JniBridge::new(self.package.as_str(), "Native", self.jni_source.clone())?,
            ),
        ))
    }

    /// Returns the Kotlin package name.
    pub fn package(&self) -> &KotlinPackage {
        &self.package
    }

    /// Returns the generated Kotlin file name.
    pub fn file(&self) -> &KotlinFile {
        &self.file
    }

    fn native_libraries(&self) -> &NativeLibraries {
        &self.native_libraries
    }

    fn api_layout(&self) -> KotlinApiStyle {
        self.api_style
    }

    fn generated_type(&self, name: TypeName) -> TypeName {
        match self.api_layout() {
            KotlinApiStyle::TopLevel => name,
            KotlinApiStyle::ModuleObject => TypeName::qualified(self.file(), name),
        }
    }

    fn factory_layout(&self) -> KotlinFactoryStyle {
        self.factory_style
    }

    fn custom_type_name(mapping: &KotlinCustomMapping) -> TypeName {
        TypeName::new(
            match (mapping.conversion(), mapping.target_type().as_str()) {
                (KotlinCustomConversion::UuidString, "UUID") => "java.util.UUID",
                (KotlinCustomConversion::UrlString, "URI") => "java.net.URI",
                _ => mapping.target_type().as_str(),
            },
        )
    }

    fn custom_type_decode(
        mapping: &KotlinCustomMapping,
        representation: Expression,
    ) -> Result<Expression> {
        match mapping.conversion() {
            KotlinCustomConversion::UuidString => Ok(Expression::invoke(
                "java.util.UUID.fromString",
                [representation].into_iter().collect::<ArgumentList>(),
            )),
            KotlinCustomConversion::UrlString => Ok(Expression::invoke(
                "java.net.URI.create",
                [representation].into_iter().collect::<ArgumentList>(),
            )),
        }
    }

    fn custom_type_encode(_mapping: &KotlinCustomMapping, value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            value,
            Identifier::parse("toString")?,
            ArgumentList::default(),
        ))
    }

    fn unsupported(shape: &'static str) -> Error {
        Error::UnsupportedTarget {
            target: Self::TARGET,
            shape,
        }
    }

    fn broken_bridge_contract(invariant: &'static str) -> Error {
        Error::BrokenBridgeContract {
            bridge: Self::TARGET,
            invariant,
        }
    }
}

impl host::HostBackend for KotlinHost {
    type Surface = Native;
    type Bridge = JniBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        Self::TARGET
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::Jni)
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
        render::Record::from_declaration(decl, self, bridge, context)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration_with_package(
            decl,
            self,
            bridge,
            context,
            Some(self.package()),
        )?
        .render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, self, bridge, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Class::from_declaration(decl, self, self.factory_layout(), bridge, context)?
            .render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Callback::from_declaration(decl, self, bridge, context)?.render()
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Stream::from_declaration(decl, self, context)?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Constant::from_declaration(decl, self, bridge, context)?.render()
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
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        render::Module::new(self, bridge, context, declarations).render()
    }
}

impl sealed::HostBackend for KotlinHost {}
