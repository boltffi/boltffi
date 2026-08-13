//! C host renderer (experimental).
//!
//! This is the C host scaffolding every renderer hangs off. C calls the shared
//! C ABI (`CBridge`) directly, so unlike a runtime bridge there is no extra
//! bridge layer stacked on top. The host's syntax fragments are the C fragments
//! the bridge already emits (`crate::bridge::c`).

pub mod name_style;
pub mod syntax;

pub use self::syntax::Syntax;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::CBridge,
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
        contract::sealed, host,
    },
};

/// C host renderer paired with the shared C ABI bridge.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CHost;

impl CHost {
    /// Creates a C host renderer.
    pub fn new() -> Self {
        Self
    }

    /// Creates the backend target stack for this C host.
    ///
    /// C calls the C ABI directly, so the stack is just `CBridge` with no
    /// additional bridge layer.
    pub fn into_target(self, _bindings: &Bindings<Native>) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(self, CBridge::default_header()?))
    }
}

impl host::HostBackend for CHost {
    type Surface = Native;
    type Bridge = crate::bridge::c::CBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "c"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .unsupported(BindingCapability::Records, "not yet implemented in C host")
            .unsupported(BindingCapability::Enums, "not yet implemented in C host")
            .unsupported(
                BindingCapability::Functions,
                "not yet implemented in C host",
            )
            .unsupported(BindingCapability::Classes, "not yet implemented in C host")
            .unsupported(
                BindingCapability::Callbacks,
                "not yet implemented in C host",
            )
            .unsupported(BindingCapability::Streams, "not yet implemented in C host")
            .unsupported(
                BindingCapability::Constants,
                "not yet implemented in C host",
            )
            .unsupported(
                BindingCapability::CustomTypes,
                "not yet implemented in C host",
            )
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        // C calls the ABI directly; only the C ABI surface is required.
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        _decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "record",
        })
    }

    fn enumeration(
        &self,
        _decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "enum",
        })
    }

    fn function(
        &self,
        _decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "function",
        })
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "class",
        })
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "callback",
        })
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "stream",
        })
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "constant",
        })
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "c",
            shape: "custom type",
        })
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        // Temporary single-file layout marker until task 02 lands the real
        // file planning. Every render method currently errors, so this is
        // never reached with declarations.
        let emitted = declarations
            .into_iter()
            .map(|declaration| declaration.into_parts().1)
            .collect::<Vec<_>>();
        crate::core::FileLayout::single(crate::core::FilePath::new("boltffi.c")?).assemble(emitted)
    }
}

impl sealed::HostBackend for CHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        core::{BindingCapability, CapabilityStatus, host::HostBackend},
        target::c::CHost,
    };

    fn empty_bindings() -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str("").expect("valid empty source"),
            PackageInfo::new("demo", None),
        )
        .expect("empty source should scan");
        lower::<Native>(&source).expect("empty source should lower")
    }

    #[test]
    fn name_is_c() {
        assert_eq!(CHost::new().name(), "c");
    }

    #[test]
    fn every_binding_capability_is_unsupported() {
        let capabilities = CHost::new().binding_capabilities();
        for capability in [
            BindingCapability::Records,
            BindingCapability::Enums,
            BindingCapability::Functions,
            BindingCapability::Classes,
            BindingCapability::Callbacks,
            BindingCapability::Streams,
            BindingCapability::Constants,
            BindingCapability::CustomTypes,
        ] {
            assert!(
                matches!(
                    capabilities.status(capability),
                    CapabilityStatus::Unsupported { .. }
                ),
                "capability {capability:?} should be unsupported for the C host"
            );
        }
    }

    #[test]
    fn into_target_succeeds() {
        let bindings = empty_bindings();
        let target = CHost::new()
            .into_target(&bindings)
            .expect("C host into_target should succeed");
        assert_eq!(target.host().name(), "c");
    }
}
