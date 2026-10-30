//! Ruby C extension host (experimental).
//!
//! The Ruby target renders a C extension that links the Rust static library
//! and calls the shared C ABI (`CBridge`) directly. Every value will cross as
//! a plain Ruby object that the extension builds eagerly, so Ruby code, and
//! YJIT, see ordinary Ruby values.
//!
//! The target renders an extension that defines the Ruby module, and the
//! `extconf.rb` that builds the Rust crate with cargo when the gem installs.
//! It also renders a starter gemspec and `lib/` entry file. The gem author
//! owns those two files after the first write. It renders no declarations yet.

mod render;
mod support;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
        contract::sealed, host,
    },
};

use self::{
    render::{extension, package::Package},
    support::unsupported,
};
use super::{
    name_style::{default_gem, default_module, extension_directory, extension_stem, package_snake},
    syntax::ConstantPath,
};

/// Ruby C extension host, paired with the shared C ABI bridge.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RubyCExtHost {
    module: Option<ConstantPath>,
    gem: Option<String>,
    library: Option<String>,
    feature_arguments: Vec<String>,
    toolchain: Option<String>,
}

impl RubyCExtHost {
    /// Creates a Ruby host renderer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects the Ruby module that the extension defines, such as
    /// `MyLib::Native`.
    pub fn module_name(mut self, module: Option<&str>) -> Result<Self> {
        self.module = module.map(ConstantPath::parse).transpose()?;
        Ok(self)
    }

    /// Selects the gem name. The extension stem is the gem name in
    /// `snake_case`.
    ///
    /// RubyGems accepts ASCII letters, digits, `.`, `-`, and `_`. A name needs
    /// a letter and starts with a letter or digit. The check also keeps the
    /// gemspec path inside the output directory.
    pub fn gem_name(mut self, gem: impl Into<String>) -> Result<Self> {
        let gem = gem.into();
        let valid = gem.starts_with(|character: char| character.is_ascii_alphanumeric())
            && gem.contains(|character: char| character.is_ascii_alphabetic())
            && gem.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
            });
        if !valid {
            return Err(Error::InvalidRubyGemName { name: gem });
        }
        self.gem = Some(gem);
        Ok(self)
    }

    /// Selects the Rust library artifact name, `demo` for `libdemo.a`.
    pub fn native_library(mut self, library: impl Into<String>) -> Self {
        self.library = Some(library.into());
        self
    }

    /// Selects the Cargo feature arguments that `extconf.rb` builds the crate
    /// with, such as `--no-default-features` and `--features=ffi`.
    ///
    /// Pass the feature arguments of the generation build, so the static
    /// library exports every function that the extension calls.
    pub fn cargo_feature_arguments(
        mut self,
        arguments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.feature_arguments = arguments.into_iter().map(Into::into).collect();
        self
    }

    /// Selects the rustup toolchain that `extconf.rb` builds the crate with,
    /// such as `+nightly`. Pass the toolchain of the generation build.
    pub fn cargo_toolchain(mut self, toolchain: Option<impl Into<String>>) -> Self {
        self.toolchain = toolchain.map(Into::into);
        self
    }

    /// Creates the backend target stack for this Ruby host.
    ///
    /// Ruby calls the C ABI directly, so the stack is `CBridge` alone. The
    /// bridge writes its header beside the extension source.
    /// The header directory uses the same stem as the Ruby package files.
    pub fn into_target(self, bindings: &Bindings<Native>) -> Result<Target<Self, CBridge>> {
        let header = format!("{}/boltffi.h", extension_directory(&self.gem(bindings)));
        Ok(Target::new(self, CBridge::new(header)?))
    }

    /// The gem name: the configured name, or the Cargo package name.
    fn gem(&self, bindings: &Bindings<Native>) -> String {
        self.gem
            .clone()
            .unwrap_or_else(|| default_gem(bindings.package().name()))
    }

    fn stem(&self, bindings: &Bindings<Native>) -> String {
        extension_stem(&self.gem(bindings))
    }

    fn module(&self, bindings: &Bindings<Native>) -> Result<ConstantPath> {
        self.module
            .clone()
            .map(Ok)
            .unwrap_or_else(|| default_module(bindings.package().name()))
    }

    /// The starter gemspec version: the Cargo package version without build
    /// metadata, because RubyGems rejects `+`.
    fn gem_version<'a>(&'a self, bindings: &'a Bindings<Native>) -> &'a str {
        let version = bindings.package().version().unwrap_or("0.1.0");
        version
            .split_once('+')
            .map_or(version, |(release, _)| release)
    }
}

impl host::HostBackend for RubyCExtHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
    type Syntax = super::syntax::Syntax;

    fn name(&self) -> &'static str {
        "ruby"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .unsupported(
                BindingCapability::Records,
                "records are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Functions,
                "functions are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Enums,
                "enums are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Classes,
                "classes are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Callbacks,
                "callbacks are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Streams,
                "streams are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Constants,
                "constants are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::CustomTypes,
                "custom types are not implemented in the Ruby host",
            )
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        _decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("record")
    }

    fn enumeration(
        &self,
        _decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("enum")
    }

    fn function(
        &self,
        _decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("function")
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("class")
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("callback")
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("stream")
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("constant")
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("custom type")
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        _declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let gem = self.gem(bindings);
        let stem = self.stem(bindings);
        let module = self.module(bindings)?;
        let artifact = self
            .library
            .clone()
            .unwrap_or_else(|| package_snake(bindings.package().name()));
        let version = self.gem_version(bindings);
        let crate_name = bindings.package().name().as_path_string();
        let package = Package {
            gem: &gem,
            version,
            stem: &stem,
            module: &module,
            artifact: &artifact,
            crate_name: &crate_name,
            feature_arguments: &self.feature_arguments,
            toolchain: self.toolchain.as_deref(),
        };
        let mut files = vec![extension::render(&gem, &stem, &module)?];
        files.extend(package.render()?);
        Ok(GeneratedOutput::new(files, Vec::new()))
    }
}

impl sealed::HostBackend for RubyCExtHost {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{CanonicalName, NamePart, lower};

    use super::*;
    use crate::core::CapabilityStatus;

    fn bindings(source: &str) -> Bindings<Native> {
        bindings_with_version(source, Some("1.2.3"))
    }

    fn bindings_with_version(source: &str, version: Option<&str>) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", version.map(str::to_owned)),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn render(host: RubyCExtHost, source: &str) -> GeneratedOutput {
        let bindings = bindings(source);
        host.into_target(&bindings)
            .expect("Ruby target")
            .render(&bindings)
            .expect("Ruby target should render")
    }

    fn file<'output>(output: &'output GeneratedOutput, path: &str) -> &'output str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new(path))
            .map(|file| file.contents())
            .unwrap_or_else(|| panic!("generated file {path}"))
    }

    fn paths(output: &GeneratedOutput) -> Vec<String> {
        output
            .files()
            .iter()
            .map(|file| file.path().as_path().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn cargo_build_metadata_does_not_change_the_gem_release() {
        for (cargo, expected) in [
            ("1.2.3", "1.2.3"),
            ("1.2.3+ci.123", "1.2.3"),
            ("1.2.3-rc.1+ci.123", "1.2.3-rc.1"),
        ] {
            let bindings = bindings_with_version("", Some(cargo));
            assert_eq!(RubyCExtHost::new().gem_version(&bindings), expected);
        }
    }

    #[test]
    fn multipart_package_names_share_one_extension_stem() {
        let mut serialized = serde_json::to_value(bindings("")).unwrap();
        serialized["package"]["name"] = serde_json::to_value(CanonicalName::new(vec![
            NamePart::new("my"),
            NamePart::new("lib"),
        ]))
        .unwrap();
        let bindings: Bindings<Native> = serde_json::from_value(serialized).unwrap();
        let output = RubyCExtHost::new()
            .into_target(&bindings)
            .unwrap()
            .render(&bindings)
            .unwrap();
        assert_eq!(
            paths(&output),
            [
                "ext/my_lib/boltffi.h",
                "ext/my_lib/my_lib.c",
                "lib/my_lib.rb",
                "ext/my_lib/extconf.rb",
                "my_lib.gemspec",
            ]
        );
    }

    #[test]
    fn complete_rendering_rejects_unsupported_functions() {
        let bindings = bindings("#[export] pub fn echo(value: u32) -> u32 { value }");
        let target = RubyCExtHost::new().into_target(&bindings).unwrap();
        assert!(matches!(
            target.render(&bindings),
            Err(Error::BindingCapability {
                target: "ruby",
                capability: BindingCapability::Functions,
                status: CapabilityStatus::Unsupported { .. },
            })
        ));
    }

    #[test]
    fn partial_rendering_reports_each_unsupported_declaration() {
        let bindings = bindings(
            r#"
            #[data]
            pub struct Point { pub x: i32, pub y: i32 }
            #[export]
            pub fn echo(value: u32) -> u32 { value }
            "#,
        );
        let output = RubyCExtHost::new()
            .into_target(&bindings)
            .unwrap()
            .render_partial(&bindings)
            .unwrap();
        assert!(!output.coverage().is_complete());
        let mut unsupported: Vec<_> = output
            .coverage()
            .unsupported()
            .iter()
            .map(|entry| (entry.declaration().kind(), entry.declaration().name()))
            .collect();
        unsupported.sort_unstable();
        assert_eq!(unsupported, [("function", "echo"), ("record", "point")]);
    }

    #[test]
    fn ruby_package_files_use_the_configured_gem_crate_and_module() {
        let host = RubyCExtHost::new()
            .gem_name("my-lib")
            .and_then(|host| host.module_name(Some("MyLib::Native")))
            .expect("valid gem and module")
            .native_library("my_lib")
            .cargo_feature_arguments(["--no-default-features", "--features=ffi,chrono"])
            .cargo_toolchain(Some("+nightly"));
        let output = render(host, "");

        assert_eq!(
            paths(&output),
            [
                "ext/my_lib/boltffi.h",
                "ext/my_lib/my_lib.c",
                "lib/my_lib.rb",
                "ext/my_lib/extconf.rb",
                "my-lib.gemspec",
            ]
        );
        insta::assert_snapshot!(
            "ruby_package_files",
            [
                "ext/my_lib/my_lib.c",
                "lib/my_lib.rb",
                "ext/my_lib/extconf.rb",
                "my-lib.gemspec",
            ]
            .map(|path| format!("==> {path}\n{}", file(&output, path)))
            .join("\n")
        );
    }

    #[test]
    fn gem_names_follow_rubygems_rules() {
        for gem in ["demo", "my-lib", "my_lib", "my.lib", "3d-demo"] {
            assert!(RubyCExtHost::new().gem_name(gem).is_ok(), "{gem}");
        }
        for gem in ["", "_demo", "-demo", ".demo", "1234", "my gem", "../evil"] {
            assert!(
                matches!(
                    RubyCExtHost::new().gem_name(gem),
                    Err(Error::InvalidRubyGemName { name }) if name == gem
                ),
                "{gem}"
            );
        }
    }
}
