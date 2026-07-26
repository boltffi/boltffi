//! Java target rendered through the shared JNI bridge.

mod admission;
mod codec;
mod name_style;
mod primitive;
mod render;
mod runtime;
mod syntax;
mod version;

use std::path::PathBuf;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, DeclarationRef, EnumDecl,
    FunctionDecl, Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::{
        c::CBridge,
        jni::{JniBridge, JniBridgeContract},
    },
    core::{
        BindingCapability, BridgeCapability, BridgeLayer, CapabilityRequirements, CoverageMode,
        CoverageReport, Emitted, Error, GeneratedOutput, HostCapabilities, RenderContext,
        RenderedDeclaration, Result, Target, contract::sealed, host,
    },
    target::jvm::{LibraryName, NativeLibraries},
};

pub use crate::target::jvm::DesktopLoader as JavaDesktopLoader;
pub use name_style::{JavaFile, JavaPackage};
use render::{Call, Callback, Class, Enumeration, ErasedSignature, Module, Record};
use syntax::{Syntax, TypeIdentifier};
pub use version::JavaVersion;

/// Java host renderer for one generated JNI owner class.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct JavaHost {
    package: JavaPackage,
    file: JavaFile,
    runtime_file: JavaFile,
    c_header: PathBuf,
    jni_source: PathBuf,
    native_libraries: NativeLibraries,
    java_version: JavaVersion,
}

impl JavaHost {
    const NATIVE_CLASS: &'static str = "Native";
    const RUNTIME_CLASS: &'static str = "BoltFFINativeRuntime";
    const TARGET: &'static str = "java";

    /// Creates a Java 8 host renderer.
    pub fn new(package: impl Into<String>, file: impl Into<String>) -> Result<Self> {
        Self::for_version(package, file, JavaVersion::default())
    }

    /// Creates a host renderer using a Java release's lexical rules.
    pub fn for_version(
        package: impl Into<String>,
        file: impl Into<String>,
        version: JavaVersion,
    ) -> Result<Self> {
        let package = JavaPackage::parse_for(package, version)?;
        let file = JavaFile::parse_for(file, version)?;
        let runtime_file = JavaFile::parse_for(Self::RUNTIME_CLASS, version)?;
        if file.as_str() == Self::NATIVE_CLASS
            || file.as_str().eq_ignore_ascii_case(runtime_file.as_str())
        {
            return Err(Error::JavaNameCollision {
                scope: package.to_string(),
                name: file.as_str().to_owned(),
            });
        }
        Ok(Self {
            package,
            file,
            runtime_file,
            c_header: PathBuf::from("jni/boltffi.h"),
            jni_source: PathBuf::from("jni/jni_glue.c"),
            native_libraries: NativeLibraries::boltffi()?,
            java_version: version,
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

    /// Selects the desktop fallback native library load name.
    pub fn desktop_fallback_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries = self
            .native_libraries
            .with_desktop_fallback(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop native-library loading policy.
    pub fn desktop_loader(mut self, loader: JavaDesktopLoader) -> Self {
        self.native_libraries = self.native_libraries.with_desktop_loader(loader);
        self
    }

    /// Selects the minimum Java source and runtime release.
    pub fn version(mut self, version: JavaVersion) -> Result<Self> {
        self.validate_version(version)?;
        self.java_version = version;
        Ok(self)
    }

    fn target(&self) -> Result<Target<Self, BridgeLayer<CBridge, JniBridge>>> {
        self.validate_version(self.java_version)?;
        self.validate_native_libraries()?;
        let c_bridge = CBridge::new(self.c_header.clone())?;
        let jni_bridge = JniBridge::new(
            self.package.to_string(),
            Self::NATIVE_CLASS,
            self.jni_source.clone(),
        )?;
        Ok(Target::new(
            self.clone(),
            BridgeLayer::new(c_bridge, jni_bridge),
        ))
    }

    /// Renders Java bindings with the requested coverage policy.
    pub fn render_with_coverage(
        &self,
        bindings: &Bindings<Native>,
        mode: crate::core::CoverageMode,
    ) -> Result<GeneratedOutput> {
        match mode {
            CoverageMode::Complete => self.target()?.render(bindings),
            CoverageMode::Partial => {
                let (bindings, coverage) = admission::Selection::new(self, bindings)?.into_parts();
                self.target()?
                    .render_partial(&bindings)
                    .map(|output| output.with_coverage(coverage))
            }
        }
    }

    /// Returns the Java package name.
    pub fn package(&self) -> &JavaPackage {
        &self.package
    }

    /// Returns the generated Java file name.
    pub fn file(&self) -> &JavaFile {
        &self.file
    }

    /// Returns the selected Java source and runtime release.
    pub const fn java_version(&self) -> JavaVersion {
        self.java_version
    }

    fn native_owner(&self) -> TypeIdentifier {
        TypeIdentifier::known(Self::NATIVE_CLASS, self.java_version)
    }

    fn runtime_owner(&self) -> TypeIdentifier {
        TypeIdentifier::known(Self::RUNTIME_CLASS, self.java_version)
    }

    fn runtime_file(&self) -> &JavaFile {
        &self.runtime_file
    }

    fn native_libraries(&self) -> &NativeLibraries {
        &self.native_libraries
    }

    fn validate_version(&self, version: JavaVersion) -> Result<()> {
        self.package.validate(version)?;
        self.file.validate(version)?;
        self.runtime_file.validate(version)
    }

    fn validate_native_libraries(&self) -> Result<()> {
        let preferred = self.native_libraries.desktop_jni().as_str();
        let fallback = self.native_libraries.desktop_fallback().as_str();
        match preferred != fallback && preferred.eq_ignore_ascii_case(fallback) {
            true => Err(Error::JavaNameCollision {
                scope: "desktop native libraries".to_owned(),
                name: fallback.to_owned(),
            }),
            false => Ok(()),
        }
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

    fn function_plan(
        &self,
        declaration: &FunctionDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Call> {
        Call::from_function(
            declaration,
            bridge,
            &self.native_owner(),
            self.java_version,
            context,
        )
    }

    fn record_plan(
        &self,
        declaration: &RecordDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Record> {
        Record::from_declaration(
            declaration,
            bridge,
            &self.native_owner(),
            self.java_version,
            context,
        )
    }

    fn enum_plan(
        &self,
        declaration: &EnumDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Enumeration> {
        Enumeration::from_declaration(
            declaration,
            bridge,
            &self.native_owner(),
            self.package(),
            self.java_version,
            context,
        )
    }

    fn class_plan(
        &self,
        declaration: &ClassDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Class> {
        Class::from_declaration(
            declaration,
            bridge,
            &self.native_owner(),
            self.java_version,
            context,
        )
    }

    fn callback_plan(
        &self,
        declaration: &CallbackDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Callback> {
        Callback::from_declaration(declaration, bridge, self.java_version, context)
    }

    fn validate_signatures(
        &self,
        bindings: &Bindings<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<()> {
        let functions = bindings
            .decls()
            .iter()
            .filter_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function) => Some(function),
                _ => None,
            })
            .map(|function| {
                self.function_plan(function, bridge, context)
                    .map(|call| call.signature().erased())
            })
            .collect::<Result<Vec<_>>>()?;
        ErasedSignature::validate_owner(self.file(), &functions)?;
        bindings.decls().iter().try_for_each(|declaration| {
            match DeclarationRef::from(declaration) {
                DeclarationRef::Record(declaration) => {
                    let record = self.record_plan(declaration, bridge, context)?;
                    let signatures = record
                        .initializers()
                        .iter()
                        .chain(record.static_methods())
                        .chain(record.instance_methods())
                        .map(Call::signature)
                        .map(|signature| signature.erased())
                        .collect::<Vec<_>>();
                    ErasedSignature::validate_owner(
                        &Record::file_for(declaration, self.java_version)?,
                        &signatures,
                    )
                }
                DeclarationRef::Enum(declaration) => {
                    let enumeration = self.enum_plan(declaration, bridge, context)?;
                    let signatures = enumeration
                        .calls()
                        .iter()
                        .map(Call::signature)
                        .map(|signature| signature.erased())
                        .collect::<Vec<_>>();
                    ErasedSignature::validate_owner(
                        &Enumeration::file_for(declaration, self.java_version)?,
                        &signatures,
                    )
                }
                DeclarationRef::Class(declaration) => {
                    let class = self.class_plan(declaration, bridge, context)?;
                    let signatures = class.signatures();
                    ErasedSignature::validate_owner(
                        &Class::file_for(declaration, self.java_version)?,
                        &signatures,
                    )
                }
                DeclarationRef::Callback(declaration) => {
                    let callback = self.callback_plan(declaration, bridge, context)?;
                    let signatures = callback.signatures();
                    ErasedSignature::validate_owner(
                        &Callback::file_for(declaration, self.java_version)?,
                        &signatures,
                    )
                }
                _ => Ok(()),
            }
        })
    }

    fn capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::CustomTypes)
            .unsupported(
                BindingCapability::Constants,
                "Java constant migration is pending",
            )
    }
}

impl host::HostBackend for JavaHost {
    type Surface = Native;
    type Bridge = JniBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        Self::TARGET
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        self.capabilities()
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::Jni)
    }

    fn preflight_coverage(
        &self,
        bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<CoverageReport> {
        self.validate_signatures(bindings, bridge, context)?;
        Ok(CoverageReport::new())
    }

    fn record(
        &self,
        declaration: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.record_plan(declaration, bridge, context)?
            .render(self.package())
    }

    fn enumeration(
        &self,
        declaration: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.enum_plan(declaration, bridge, context)?
            .render(self.package())
    }

    fn function(
        &self,
        declaration: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.function_plan(declaration, bridge, context)?.render()
    }

    fn class(
        &self,
        declaration: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.class_plan(declaration, bridge, context)?
            .render(self.package())
    }

    fn callback(
        &self,
        declaration: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.callback_plan(declaration, bridge, context)?
            .render(self.package())
    }

    fn stream(
        &self,
        declaration: &StreamDecl<Self::Surface>,
        _: &Self::Bridge,
        _: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        admission::StreamShape::classify(declaration).require_supported()?;
        Ok(Emitted::primary(""))
    }

    fn constant(
        &self,
        _: &ConstantDecl<Self::Surface>,
        _: &Self::Bridge,
        _: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Self::unsupported("constant declaration"))
    }

    fn custom_type(
        &self,
        _: &CustomTypeDecl,
        _: &Self::Bridge,
        _: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::primary(""))
    }

    fn assemble<'decl>(
        &self,
        _: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        Module::new(self, bridge, context, declarations).render()
    }
}

impl sealed::HostBackend for JavaHost {}
