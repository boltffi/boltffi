//! Ruby C extension host (experimental).
//!
//! The Ruby target renders a C extension that links the Rust static library
//! and calls the shared C ABI (`CBridge`) directly. Every value will cross as
//! a plain Ruby object that the extension builds eagerly, so Ruby code, and
//! YJIT, see ordinary Ruby values.
//!
//! This scaffold declares the host, its syntax family, and the extension
//! stem. The plumbing and the renderer follow in later changes.

mod support;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, GeneratedOutput,
        HostCapabilities, RenderContext, RenderedDeclaration, Result, Target, contract::sealed,
        host,
    },
};

use self::support::unsupported;
use super::name_style::package_snake;

/// Ruby C extension host, paired with the shared C ABI bridge.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RubyCExtHost;

impl RubyCExtHost {
    /// Creates a Ruby host renderer.
    pub fn new() -> Self {
        Self
    }

    /// Creates the backend target stack for this Ruby host.
    ///
    /// Ruby calls the C ABI directly, so the stack is `CBridge` alone. The
    /// bridge writes its header beside the extension source.
    /// The header directory uses the same stem as the Ruby package files.
    pub fn into_target(self, bindings: &Bindings<Native>) -> Result<Target<Self, CBridge>> {
        let stem = package_snake(bindings.package().name());
        let header = format!("ext/{stem}/boltffi.h");
        Ok(Target::new(self, CBridge::new(header)?))
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
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        _declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        // Every declaration is unsupported until the renderer lands, so there
        // is nothing to assemble yet.
        Ok(GeneratedOutput::empty())
    }
}

impl sealed::HostBackend for RubyCExtHost {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{CanonicalName, NamePart, lower};

    use super::*;
    use crate::core::{CapabilityStatus, Error};

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Native>(&source).expect("source lowers")
    }

    #[test]
    fn header_path_uses_the_multipart_package_stem() {
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
        let paths: Vec<_> = output
            .files()
            .iter()
            .map(|file| file.path().as_path())
            .collect();
        assert_eq!(paths, [Path::new("ext/my_lib/boltffi.h")]);
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
        let paths: Vec<_> = output
            .files()
            .iter()
            .map(|file| file.path().as_path())
            .collect();
        assert_eq!(paths, [Path::new("ext/demo/boltffi.h")]);
    }
}
