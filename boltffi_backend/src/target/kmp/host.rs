use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::core::{
    BindingCapability, BridgeCapability, CapabilityRequirements, CoverageMode, CoverageReport,
    DeclarationLabel, Diagnostic, Emitted, GeneratedOutput, HostCapabilities, RenderContext,
    RenderedDeclaration, Result, Target, UnsupportedDeclaration, contract::sealed, host,
};

use super::{
    KmpBridge, KmpBridgeContract, KmpEmissionOptions, KmpEmitter, KmpPlatform, KmpSupportMode,
    Syntax,
    lower::{KmpLowerer, KmpLoweringOptions, KmpSupportPlan},
};

/// Kotlin Multiplatform host renderer for the IR backend plan.
///
/// The host lowers to a typed [`super::KmpModule`] plan before file emission.
/// Complete coverage rendering remains strict: APIs outside the selected
/// platform capability intersection or current body-emission ownership produce
/// diagnostics that the backend driver turns into generation failures.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpHost {
    selected_platforms: Vec<KmpPlatform>,
    support_mode: KmpSupportMode,
    package_name: String,
    module_name: String,
    min_sdk: u32,
}

impl KmpHost {
    /// Creates a KMP host renderer.
    pub fn new() -> Self {
        Self {
            selected_platforms: KmpPlatform::default_selected(),
            support_mode: KmpSupportMode::Strict,
            package_name: "com.example.boltffi".to_string(),
            module_name: "BoltFFI".to_string(),
            min_sdk: 24,
        }
    }

    /// Selects the KMP platform matrix checked by admission.
    pub fn selected_platforms(mut self, platforms: impl Into<Vec<KmpPlatform>>) -> Self {
        self.selected_platforms = platforms.into();
        self
    }

    /// Sets the support mode recorded by emitted KMP support metadata.
    pub fn support_mode(mut self, support_mode: KmpSupportMode) -> Self {
        self.support_mode = support_mode;
        self
    }

    /// Sets the Kotlin package used for common and platform source sets.
    pub fn package_name(mut self, package_name: impl Into<String>) -> Self {
        self.package_name = package_name.into();
        self
    }

    /// Sets the generated Kotlin module/source class name.
    pub fn module_name(mut self, module_name: impl Into<String>) -> Self {
        self.module_name = module_name.into();
        self
    }

    /// Sets the Android minSdk written into generated Gradle output.
    pub fn min_sdk(mut self, min_sdk: u32) -> Self {
        self.min_sdk = min_sdk;
        self
    }

    /// Creates the backend target stack for this skeletal KMP host.
    pub fn into_target(self) -> Target<Self, KmpBridge> {
        Target::new(self, KmpBridge)
    }

    fn emit_declaration_placeholder(&self) -> Emitted {
        Emitted::primary("")
    }

    fn support_plan(&self, bindings: &Bindings<Native>) -> KmpSupportPlan {
        KmpLowerer::new(
            KmpLoweringOptions::new().selected_platforms(self.selected_platforms.clone()),
        )
        .support_plan(bindings)
    }
}

fn coverage_from_support_plan(support_plan: &KmpSupportPlan) -> CoverageReport {
    let mut coverage = CoverageReport::new();
    for declaration in support_plan.declarations() {
        for message in support_messages(declaration) {
            coverage.push(UnsupportedDeclaration::new(
                declaration.label().clone(),
                message,
            ));
        }
    }
    coverage
}

fn diagnostics_from_support_plan(support_plan: &KmpSupportPlan) -> Vec<Diagnostic> {
    support_plan
        .declarations()
        .iter()
        .flat_map(support_messages)
        .map(Diagnostic::new)
        .collect()
}

fn support_messages(declaration: &super::lower::KmpDeclarationSupport) -> Vec<String> {
    declaration
        .records()
        .iter()
        .filter_map(|record| {
            record.reason().map(|reason| {
                api_message(declaration.label(), record.kind(), record.name(), reason)
            })
        })
        .collect()
}

fn api_message(label: &DeclarationLabel, kind: &str, name: &str, reason: &str) -> String {
    if kind == label.kind() && name == label.name() {
        reason.to_owned()
    } else {
        format!("{kind} {name}: {reason}")
    }
}

impl Default for KmpHost {
    fn default() -> Self {
        Self::new()
    }
}

impl host::HostBackend for KmpHost {
    type Surface = Native;
    type Bridge = KmpBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "kotlin_multiplatform"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new()
    }

    fn preflight_coverage(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<CoverageReport> {
        Ok(coverage_from_support_plan(&self.support_plan(bindings)))
    }

    fn record(
        &self,
        _decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn enumeration(
        &self,
        _decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn function(
        &self,
        _decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_declaration_placeholder())
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        _declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let support_plan = self.support_plan(bindings);
        let diagnostics = if matches!(context.coverage_mode(), CoverageMode::Partial) {
            diagnostics_from_support_plan(&support_plan)
        } else {
            Vec::new()
        };
        let support_mode = if matches!(context.coverage_mode(), CoverageMode::Partial)
            && support_plan.has_rejections()
        {
            KmpSupportMode::PreviewPruneUnsupported
        } else {
            self.support_mode
        };
        let module = KmpLowerer::new(
            KmpLoweringOptions::new()
                .selected_platforms(self.selected_platforms.clone())
                .support_mode(support_mode),
        )
        .lower_support_plan(support_plan)
        .map_err(|error| error.into_backend_error())?;
        let emitted = KmpEmitter::new(KmpEmissionOptions::new(
            self.package_name.clone(),
            self.module_name.clone(),
            self.min_sdk,
        ))
        .emit(&module)?;
        let (files, mut emitted_diagnostics, _coverage) = emitted.into_parts();
        emitted_diagnostics.extend(diagnostics);
        Ok(GeneratedOutput::new(files, emitted_diagnostics))
    }
}

impl sealed::HostBackend for KmpHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        Error,
        target::kmp::{KMP_SUPPORT_REPORT_FILE, KmpHost, KmpPlatform},
    };

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source fixture"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn output_paths(output: &crate::GeneratedOutput) -> Vec<String> {
        output
            .files()
            .iter()
            .map(|file| file.path().as_path().display().to_string())
            .collect()
    }

    fn file<'output>(output: &'output crate::GeneratedOutput, path: &str) -> &'output str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new(path))
            .unwrap_or_else(|| panic!("missing generated file {path}"))
            .contents()
    }

    fn expected_default_file_list() -> Vec<&'static str> {
        vec![
            "settings.gradle.kts",
            "build.gradle.kts",
            "src/commonMain/kotlin/com/example/boltffi/BoltFFI.kt",
            KMP_SUPPORT_REPORT_FILE,
            "src/jvmMain/kotlin/com/example/boltffi/BoltFFIJvmActual.kt",
            "src/androidMain/kotlin/com/example/boltffi/BoltFFIAndroidActual.kt",
            "src/jvmMain/kotlin/com/example/boltffi/jvm/BoltFFI.kt",
            "src/androidMain/kotlin/com/example/boltffi/jvm/BoltFFI.kt",
            "src/jvmMain/c/jni_glue.c",
            "src/androidMain/c/jni_glue.c",
        ]
    }

    #[test]
    fn kmp_target_renders_empty_surface_file_list() {
        let output = KmpHost::new()
            .into_target()
            .render(&bindings(""))
            .expect("empty KMP IR plan should render project files");

        assert_eq!(output_paths(&output), expected_default_file_list());
        assert!(output.diagnostics().is_empty());
        assert!(output.coverage().is_complete());
    }

    #[test]
    fn kmp_target_rejects_sync_primitive_function_until_jni_glue_is_delegated() {
        let error = KmpHost::new()
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect_err("primitive sync functions need delegated JNI glue before generation");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function add"), "{reason}");
                assert!(reason.contains("JNI glue emission"), "{reason}");
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_partial_prunes_sync_function_rejected_by_body_emission() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("partial KMP generation should prune unrenderable function bodies");

        assert_eq!(output_paths(&output), expected_default_file_list());
        assert!(!output.coverage().is_complete());
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message().contains("JNI glue emission"))
        );

        let common = file(
            &output,
            "src/commonMain/kotlin/com/example/boltffi/BoltFFI.kt",
        );
        assert!(!common.contains("expect fun add"));

        let report: serde_json::Value =
            serde_json::from_str(file(&output, KMP_SUPPORT_REPORT_FILE))
                .expect("valid support report");
        assert_eq!(report["admitted_apis"], serde_json::json!([]));
        assert_eq!(report["rejected_apis"][0]["kind"], "function");
        assert_eq!(report["rejected_apis"][0]["name"], "add");
        assert!(
            report["rejected_apis"][0]["reason"]
                .as_str()
                .expect("reason")
                .contains("JNI glue emission")
        );
    }

    #[test]
    fn kmp_target_partial_reports_support_metadata_reasons_for_duplicate_function_declarations() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
                r#"
                #[export]
                pub fn ping_pong(value: i32) -> i32 {
                    value
                }

                #[export]
                pub fn ping__pong(value: i32) -> i32 {
                    value
                }
                "#,
            ))
            .expect("partial KMP generation should prune duplicate unrenderable functions");
        let diagnostic_messages = output
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message())
            .collect::<Vec<_>>();
        let unsupported_reasons = output
            .coverage()
            .unsupported()
            .iter()
            .map(|unsupported| unsupported.reason())
            .collect::<Vec<_>>();
        let report: serde_json::Value =
            serde_json::from_str(file(&output, KMP_SUPPORT_REPORT_FILE))
                .expect("valid support report");
        let rejected_reasons = report["rejected_apis"]
            .as_array()
            .expect("rejected APIs")
            .iter()
            .map(|api| {
                api["reason"]
                    .as_str()
                    .expect("support report rejection reason")
            })
            .collect::<Vec<_>>();

        assert_eq!(diagnostic_messages.len(), 2);
        assert_eq!(output.coverage().unsupported().len(), 2);
        assert_eq!(rejected_reasons.len(), 2);
        for reasons in [diagnostic_messages, unsupported_reasons, rejected_reasons] {
            assert!(
                reasons
                    .iter()
                    .any(|reason| reason.contains("JNI glue emission")),
                "{reasons:#?}"
            );
            assert!(
                reasons
                    .iter()
                    .any(|reason| reason.contains("duplicate Kotlin function signature")),
                "{reasons:#?}"
            );
        }
    }

    #[test]
    fn kmp_target_rejects_unsigned_primitive_function_until_jni_carrier_plan_exists() {
        let error = KmpHost::new()
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn round_trip(value: u32) -> u32 {
                    value
                }
                "#,
            ))
            .expect_err("unsigned primitives need an explicit JNI carrier plan");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function round::trip"), "{reason}");
                assert!(
                    reason.contains("unsigned primitive JNI carrier"),
                    "{reason}"
                );
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_uses_configured_output_identity_in_files_and_metadata() {
        let output = KmpHost::new()
            .package_name("com.acme.demo")
            .module_name("Demo")
            .min_sdk(26)
            .into_target()
            .render(&bindings(""))
            .expect("empty KMP IR plan should render project files");
        let paths = output_paths(&output);
        let report = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new(KMP_SUPPORT_REPORT_FILE))
            .expect("support report");
        let json: serde_json::Value =
            serde_json::from_str(report.contents()).expect("valid support metadata");

        assert!(paths.contains(&"src/commonMain/kotlin/com/acme/demo/Demo.kt".to_string()));
        assert!(paths.contains(&"src/jvmMain/kotlin/com/acme/demo/DemoJvmActual.kt".to_string()));
        assert_eq!(json["package_name"], "com.acme.demo");
        assert_eq!(json["module_name"], "Demo");
        assert_eq!(json["min_sdk"], 26);
        assert_eq!(json["mode"], "strict");
    }

    #[test]
    fn kmp_target_rejects_apis_outside_platform_intersection() {
        let error = KmpHost::new()
            .selected_platforms(vec![KmpPlatform::Jvm, KmpPlatform::IosSimulatorArm64])
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect_err("KMP IR plan should reject APIs not supported by every platform");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function add"));
                assert!(reason.contains("synchronous callables on iosSimulatorArm64"));
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_empty_platform_matrix() {
        let error = KmpHost::new()
            .selected_platforms(Vec::<KmpPlatform>::new())
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect_err("KMP IR plan should require at least one selected platform");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function add"), "{reason}");
                assert!(reason.contains("no selected KMP platforms"), "{reason}");
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_empty_platform_matrix_without_apis() {
        let error = KmpHost::new()
            .selected_platforms(Vec::<KmpPlatform>::new())
            .into_target()
            .render(&bindings(""))
            .expect_err("KMP IR plan should require at least one selected platform");

        match error {
            Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "invalid KMP platform matrix",
            } => {}
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_non_default_platform_matrix_until_emission_is_parameterized() {
        let error = KmpHost::new()
            .selected_platforms(vec![KmpPlatform::Jvm])
            .into_target()
            .render(&bindings(""))
            .expect_err("empty JVM-only KMP plans must not emit JVM+Android files");

        match error {
            Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "non-default KMP platform emission",
            } => {}
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_api_using_custom_type_with_unsupported_representation() {
        let error = KmpHost::new()
            .into_target()
            .render(&bindings(
                r#"
                use std::time::Duration;

                custom_type!(
                    BadDuration,
                    remote = RemoteDuration,
                    repr = Duration,
                    into_ffi = |_value: &RemoteDuration| Duration::from_secs(0),
                    try_from_ffi = |_value: Duration| Ok(RemoteDuration),
                );

                #[export]
                pub fn echo_bad(value: RemoteDuration) -> RemoteDuration {
                    value
                }
                "#,
            ))
            .expect_err("KMP IR plan should reject custom type representations it cannot admit");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function echo::bad"), "{reason}");
                assert!(reason.contains("unknown binding shapes on jvm"), "{reason}");
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_reports_unsupported_apis_in_partial_mode() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
                r#"
                pub struct Engine;

                #[export(single_threaded)]
                impl Engine {
                    pub fn new() -> Self {
                        Engine
                    }
                }
                "#,
            ))
            .expect("partial KMP IR skeleton should report unsupported APIs");
        let unsupported = output.coverage().unsupported();

        assert_eq!(output_paths(&output), expected_default_file_list());
        assert_eq!(output.diagnostics().len(), 2);
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message()
                    == "unsupported classes on jvm, classes on android")
        );
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message()
                    == "class initializer engine::new: unsupported classes on jvm, classes on android")
        );
        assert_eq!(unsupported.len(), 2);
        assert_eq!(unsupported[0].declaration().kind(), "class");
        assert_eq!(unsupported[0].declaration().name(), "engine");
        assert_eq!(
            unsupported[0].reason(),
            "unsupported classes on jvm, classes on android"
        );
    }

    #[test]
    fn kmp_target_reports_every_unsupported_owned_api_in_partial_mode() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
                r#"
                pub struct Engine;

                #[export(single_threaded)]
                impl Engine {
                    pub async fn load(&self) -> i32 {
                        1
                    }

                    pub async fn save(&self) -> i32 {
                        2
                    }
                }
                "#,
            ))
            .expect("partial KMP IR skeleton should report every unsupported owned API");
        let unsupported = output.coverage().unsupported();
        let diagnostics = output.diagnostics();

        assert_eq!(output_paths(&output), expected_default_file_list());
        assert_eq!(diagnostics.len(), 3);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("class method engine::load"))
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("class method engine::save"))
        );
        assert_eq!(unsupported.len(), 3);
    }
}
