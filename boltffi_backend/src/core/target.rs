use boltffi_binding::{Bindings, Decl, DeclarationRef, Surface};

use crate::core::{
    BindingCapability, BridgeContract, CapabilityRequirements, CoverageMode, CoverageReport,
    DeclarationLabel, Error, GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration,
    Result, UnsupportedDeclaration, bridge, contract::sealed, host,
};

/// A bridge layer stacked above another bridge stack.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct BridgeLayer<Lower, Upper> {
    lower: Lower,
    upper: Upper,
}

impl<Lower, Upper> BridgeLayer<Lower, Upper> {
    /// Creates a layered bridge stack.
    pub const fn new(lower: Lower, upper: Upper) -> Self {
        Self { lower, upper }
    }

    /// Returns the lower bridge stack.
    pub const fn lower(&self) -> &Lower {
        &self.lower
    }

    /// Returns the upper bridge layer.
    pub const fn upper(&self) -> &Upper {
        &self.upper
    }
}

impl<B, S> sealed::BridgeStack for B
where
    B: bridge::BridgeBackend<Input = Bindings<S>, Surface = S>,
    S: Surface,
{
}

impl<B, S> bridge::BridgeStack for B
where
    B: bridge::BridgeBackend<Input = Bindings<S>, Surface = S>,
    S: Surface,
{
    type Surface = S;
    type Contract = B::Contract;

    fn build(
        &self,
        bindings: &Bindings<Self::Surface>,
    ) -> Result<bridge::BridgeOutput<Self::Contract>> {
        let contract = self.build_contract(bindings)?;
        let output = self.render_bridge(bindings, &contract)?;
        Ok(bridge::BridgeOutput::new(contract, output))
    }
}

impl<Lower, Upper> sealed::BridgeStack for BridgeLayer<Lower, Upper>
where
    Lower: bridge::BridgeStack,
    Upper: bridge::BridgeBackend<Input = Lower::Contract, Surface = Lower::Surface>,
{
}

impl<Lower, Upper> bridge::BridgeStack for BridgeLayer<Lower, Upper>
where
    Lower: bridge::BridgeStack,
    Upper: bridge::BridgeBackend<Input = Lower::Contract, Surface = Lower::Surface>,
{
    type Surface = Lower::Surface;
    type Contract = Upper::Contract;

    fn build(
        &self,
        bindings: &Bindings<Self::Surface>,
    ) -> Result<bridge::BridgeOutput<Self::Contract>> {
        let lower = self.lower.build(bindings)?;
        let (lower_contract, mut output) = lower.into_parts();
        let contract = self.upper.build_contract(&lower_contract)?;
        output.append(self.upper.render_bridge(&lower_contract, &contract)?);
        Ok(bridge::BridgeOutput::new(contract, output))
    }
}

/// A host renderer paired with the bridge stack it requires.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Target<H, S> {
    host: H,
    stack: S,
}

impl<H, S> Target<H, S>
where
    H: host::HostBackend<Bridge = S::Contract, Surface = S::Surface>,
    S: bridge::BridgeStack,
{
    /// Creates a target from a host renderer and bridge stack.
    pub const fn new(host: H, stack: S) -> Self {
        Self { host, stack }
    }

    /// Returns the host renderer.
    pub const fn host(&self) -> &H {
        &self.host
    }

    /// Returns the bridge stack.
    pub const fn stack(&self) -> &S {
        &self.stack
    }

    /// Renders a binding contract through the paired bridge and host.
    pub fn render(&self, bindings: &Bindings<S::Surface>) -> Result<GeneratedOutput> {
        self.render_with_coverage(bindings, CoverageMode::Complete)
    }

    /// Renders supported declarations and reports unsupported declarations.
    pub fn render_partial(&self, bindings: &Bindings<S::Surface>) -> Result<GeneratedOutput> {
        self.render_with_coverage(bindings, CoverageMode::Partial)
    }

    /// Renders a binding contract with the requested coverage policy.
    pub fn render_with_coverage(
        &self,
        bindings: &Bindings<S::Surface>,
        mode: CoverageMode,
    ) -> Result<GeneratedOutput> {
        let bridge = self.stack.build(bindings)?;
        let (contract, mut output) = bridge.into_parts();
        let host_capabilities = self.host.binding_capabilities();
        if matches!(mode, CoverageMode::Complete) {
            let binding_requirements = CapabilityRequirements::from_bindings(bindings);
            host_capabilities.require_binding(self.host.name(), &binding_requirements)?;
        }
        contract
            .capabilities()
            .require_bridge(self.host.name(), &self.host.bridge_capabilities())?;
        let context = RenderContext::new(bindings, self.host.name(), mode)
            .with_custom_type_mappings(self.host.custom_type_mappings(bindings)?);
        let preflight_coverage = self
            .host
            .preflight_coverage(bindings, &contract, &context)?;
        let (declarations, coverage) = bindings.decls().iter().try_fold(
            (Vec::new(), preflight_coverage),
            |accumulator, decl| {
                self.render_declaration_with_coverage(
                    decl,
                    &contract,
                    &context,
                    &host_capabilities,
                    mode,
                    accumulator,
                )
            },
        )?;
        if matches!(mode, CoverageMode::Complete) && !coverage.is_complete() {
            return Err(Self::coverage_error(self.host.name(), &coverage));
        }
        let host_emitted = self
            .host
            .assemble(bindings, &contract, &context, declarations)?
            .with_coverage(coverage);
        output.append(host_emitted);
        Ok(output)
    }

    fn render_declaration_with_coverage<'decl>(
        &self,
        decl: &'decl Decl<S::Surface>,
        bridge: &S::Contract,
        context: &RenderContext<S::Surface>,
        host_capabilities: &HostCapabilities,
        mode: CoverageMode,
        mut accumulator: (Vec<RenderedDeclaration<'decl, S::Surface>>, CoverageReport),
    ) -> Result<(Vec<RenderedDeclaration<'decl, S::Surface>>, CoverageReport)> {
        let declaration = DeclarationRef::from(decl);
        let label = DeclarationLabel::from_ref(declaration);
        let capability = BindingCapability::from_decl(decl);
        let status = host_capabilities.status(capability);
        if !status.is_stable() {
            if matches!(mode, CoverageMode::Complete) {
                return Err(Error::BindingCapability {
                    target: self.host.name(),
                    capability,
                    status,
                });
            }
            if !status.renderable_in_partial() {
                accumulator
                    .1
                    .push(UnsupportedDeclaration::new(label, status.reason()));
                return Ok(accumulator);
            }
        }

        match self.render_declaration(decl, bridge, context) {
            Ok(rendered) => {
                if !status.is_stable() {
                    accumulator
                        .1
                        .push(UnsupportedDeclaration::new(label.clone(), status.reason()));
                }
                rendered
                    .emitted()
                    .diagnostics()
                    .iter()
                    .for_each(|diagnostic| {
                        accumulator.1.push(UnsupportedDeclaration::new(
                            label.clone(),
                            diagnostic.message(),
                        ));
                    });
                accumulator.0.push(rendered);
                Ok(accumulator)
            }
            Err(error) if matches!(mode, CoverageMode::Partial) => match error {
                Error::UnsupportedTarget { shape, .. } | Error::UnsupportedCAbi { shape } => {
                    accumulator
                        .1
                        .push(UnsupportedDeclaration::new(label, shape));
                    Ok(accumulator)
                }
                other => Err(other),
            },
            Err(error) => Err(error),
        }
    }

    fn coverage_error(target: &'static str, coverage: &CoverageReport) -> Error {
        let reason = coverage
            .unsupported()
            .first()
            .map(|unsupported| {
                format!(
                    "{} {}: {}",
                    unsupported.declaration().kind(),
                    unsupported.declaration().name(),
                    unsupported.reason()
                )
            })
            .unwrap_or_else(|| "unknown unsupported declaration".to_owned());
        Error::IncompleteCoverage { target, reason }
    }

    fn render_declaration<'decl>(
        &self,
        decl: &'decl Decl<S::Surface>,
        bridge: &S::Contract,
        context: &RenderContext<S::Surface>,
    ) -> Result<RenderedDeclaration<'decl, S::Surface>> {
        let declaration = DeclarationRef::from(decl);
        let emitted = match declaration {
            DeclarationRef::Record(record) => self.host.record(record, bridge, context),
            DeclarationRef::Enum(enumeration) => {
                self.host.enumeration(enumeration, bridge, context)
            }
            DeclarationRef::Function(function) => self.host.function(function, bridge, context),
            DeclarationRef::Class(class) => self.host.class(class, bridge, context),
            DeclarationRef::Callback(callback) => self.host.callback(callback, bridge, context),
            DeclarationRef::Stream(stream) => self.host.stream(stream, bridge, context),
            DeclarationRef::Constant(constant) => self.host.constant(constant, bridge, context),
            DeclarationRef::CustomType(custom_type) => {
                self.host.custom_type(custom_type, bridge, context)
            }
        }?;
        Ok(RenderedDeclaration::new(declaration, emitted))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::core::{
        BindingCapability, BridgeCapabilities, BridgeCapability, BridgeContract,
        CapabilityRequirements, Emitted, Error, GeneratedOutput, HostCapabilities, LanguageSyntax,
        RenderContext, RenderedDeclaration, Result, bridge, contract::sealed, host,
        syntax::sealed as syntax_sealed,
    };

    #[derive(Clone)]
    struct TestFragment;

    impl fmt::Display for TestFragment {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test")
        }
    }

    impl syntax_sealed::SyntaxFragment for TestFragment {}

    #[derive(Clone, Copy)]
    struct TestSyntax;

    impl LanguageSyntax for TestSyntax {
        const KEYWORDS: &'static [&'static str] = &[];

        type Identifier = TestFragment;
        type Type = TestFragment;
        type Expr = TestFragment;
        type Stmt = TestFragment;
        type Literal = TestFragment;
        type Arguments = TestFragment;
    }

    impl syntax_sealed::LanguageSyntax for TestSyntax {}

    fn function_bindings() -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            )
            .expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source scans");
        lower::<Native>(&source).expect("source lowers")
    }

    #[derive(Clone)]
    struct NativeContract {
        capabilities: BridgeCapabilities,
    }

    impl BridgeContract for NativeContract {
        type Surface = Native;

        fn capabilities(&self) -> &BridgeCapabilities {
            &self.capabilities
        }
    }

    impl sealed::BridgeContract for NativeContract {}

    #[derive(Clone, Copy)]
    struct NativeBridge;

    impl bridge::BridgeBackend for NativeBridge {
        type Surface = Native;
        type Input = Bindings<Native>;
        type Contract = NativeContract;
        type Syntax = TestSyntax;

        fn build_contract(&self, _input: &Self::Input) -> Result<Self::Contract> {
            Ok(NativeContract {
                capabilities: BridgeCapabilities::new().stable(BridgeCapability::CAbi),
            })
        }

        fn render_bridge(
            &self,
            _input: &Self::Input,
            _contract: &Self::Contract,
        ) -> Result<GeneratedOutput> {
            Ok(GeneratedOutput::empty())
        }
    }

    impl sealed::BridgeBackend for NativeBridge {}

    #[derive(Clone, Copy)]
    struct SwiftHost;

    impl Emitted {
        fn placeholder() -> Self {
            Self::primary("placeholder\n")
        }
    }

    impl host::HostBackend for SwiftHost {
        type Surface = Native;
        type Bridge = NativeContract;
        type Syntax = TestSyntax;

        fn name(&self) -> &'static str {
            "swift"
        }

        fn binding_capabilities(&self) -> HostCapabilities {
            HostCapabilities::new()
        }

        fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
            CapabilityRequirements::new().require(BridgeCapability::CAbi)
        }

        fn record(
            &self,
            _decl: &boltffi_binding::RecordDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn enumeration(
            &self,
            _decl: &boltffi_binding::EnumDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn function(
            &self,
            _decl: &boltffi_binding::FunctionDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn class(
            &self,
            _decl: &boltffi_binding::ClassDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn callback(
            &self,
            _decl: &boltffi_binding::CallbackDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn stream(
            &self,
            _decl: &boltffi_binding::StreamDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn constant(
            &self,
            _decl: &boltffi_binding::ConstantDecl<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn custom_type(
            &self,
            _decl: &boltffi_binding::CustomTypeDecl,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::placeholder())
        }

        fn assemble<'decl>(
            &self,
            _bindings: &Bindings<Self::Surface>,
            _bridge: &Self::Bridge,
            _context: &RenderContext<Self::Surface>,
            _declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
        ) -> Result<GeneratedOutput> {
            Ok(GeneratedOutput::empty())
        }
    }

    impl sealed::HostBackend for SwiftHost {}

    #[test]
    fn target_accepts_host_with_matching_bridge_contract() {
        let _target = super::Target::new(SwiftHost, NativeBridge);
    }

    #[test]
    fn complete_render_rejects_missing_binding_capability() {
        let target = super::Target::new(SwiftHost, NativeBridge);
        let error = target
            .render(&function_bindings())
            .expect_err("complete render should reject unsupported function capability");

        assert!(matches!(
            error,
            Error::BindingCapability {
                target: "swift",
                capability: BindingCapability::Functions,
                ..
            }
        ));
    }

    #[test]
    fn partial_render_reports_unsupported_declarations() {
        let target = super::Target::new(SwiftHost, NativeBridge);
        let output = target
            .render_partial(&function_bindings())
            .expect("partial render should report unsupported functions");
        let unsupported = output.coverage().unsupported();

        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0].declaration().kind(), "function");
        assert_eq!(unsupported[0].declaration().name(), "add");
        assert_eq!(unsupported[0].reason(), "capability was not advertised");
    }
}
