use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::core::{
    BindingCapability, BridgeCapability, CapabilityRequirements, CoverageMode, CoverageReport,
    DeclarationLabel, Diagnostic, Emitted, GeneratedOutput, HostCapabilities, RenderContext,
    RenderedDeclaration, Result, Target, UnsupportedDeclaration, contract::sealed, host,
};
use crate::{
    bridge::jni::JniBridgeContract,
    target::jvm::{DesktopLoader, LibraryName},
};

use super::{
    KmpBridge, KmpEmissionOptions, KmpEmitter, KmpPlatform, KmpSupportMode, Syntax,
    library::KmpNativeLibraryConfiguration,
    lower::{KmpLowerer, KmpLoweringOptions, KmpSupportPlan},
};

/// Default Kotlin package used for generated KMP sources.
pub const DEFAULT_KMP_PACKAGE_NAME: &str = "com.example.boltffi";

/// Default generated Kotlin module/source class name.
pub const DEFAULT_KMP_MODULE_NAME: &str = "BoltFFI";

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
    native_library_configuration: KmpNativeLibraryConfiguration,
}

impl KmpHost {
    /// Creates a KMP host renderer.
    pub fn new() -> Self {
        Self {
            selected_platforms: KmpPlatform::default_selected(),
            support_mode: KmpSupportMode::Strict,
            package_name: DEFAULT_KMP_PACKAGE_NAME.to_string(),
            module_name: DEFAULT_KMP_MODULE_NAME.to_string(),
            min_sdk: 24,
            native_library_configuration: KmpNativeLibraryConfiguration::default(),
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

    /// Selects the Android native library load name.
    pub fn android_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_library_configuration = self
            .native_library_configuration
            .android_library(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop JNI wrapper library load name.
    pub fn desktop_jni_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_library_configuration = self
            .native_library_configuration
            .desktop_jni_library(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop fallback library load name.
    pub fn desktop_fallback_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_library_configuration = self
            .native_library_configuration
            .desktop_fallback_library(LibraryName::parse(library)?);
        Ok(self)
    }

    /// Selects the desktop native-library loading policy.
    pub fn desktop_loader(mut self, loader: DesktopLoader) -> Self {
        self.native_library_configuration =
            self.native_library_configuration.desktop_loader(loader);
        self
    }

    /// Creates the backend target stack for this KMP host.
    pub fn into_target(self) -> Target<Self, KmpBridge> {
        let internal_package = format!("{}.jvm", self.package_name);
        Target::new(self, KmpBridge::new(internal_package))
    }

    fn emit_declaration_placeholder(&self) -> Emitted {
        Emitted::primary("")
    }

    fn support_plan(
        &self,
        bindings: &Bindings<Native>,
        bridge: &JniBridgeContract,
    ) -> KmpSupportPlan {
        KmpLowerer::new(self.lowering_options(KmpSupportMode::Strict))
            .support_plan_with_bridge(bindings, bridge)
    }

    fn lowering_options(&self, support_mode: KmpSupportMode) -> KmpLoweringOptions {
        KmpLoweringOptions::new()
            .selected_platforms(self.selected_platforms.clone())
            .support_mode(support_mode)
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
    type Bridge = JniBridgeContract;
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
        CapabilityRequirements::new().require(BridgeCapability::Jni)
    }

    fn preflight_coverage(
        &self,
        bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<CoverageReport> {
        Ok(coverage_from_support_plan(
            &self.support_plan(bindings, bridge),
        ))
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
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        _declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let support_plan = self.support_plan(bindings, bridge);
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
        let module = KmpLowerer::new(self.lowering_options(support_mode))
            .lower_support_plan(support_plan)
            .map_err(|error| error.into_backend_error())?;
        let native_libraries = self.native_library_configuration.resolve(bindings)?;
        let emitted = KmpEmitter::new(
            KmpEmissionOptions::new(
                self.package_name.clone(),
                self.module_name.clone(),
                self.min_sdk,
            )
            .native_libraries(native_libraries),
        )
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
        bindings_for_package("demo", source)
    }

    fn bindings_for_package(package: &str, source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source fixture"),
            PackageInfo::new(package, None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn output_paths(output: &crate::GeneratedOutput) -> Vec<String> {
        output
            .files()
            .iter()
            .map(|file| {
                file.path()
                    .as_path()
                    .display()
                    .to_string()
                    .replace('\\', "/")
            })
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
            "src/jvmMain/c/jni_glue.c",
            "src/androidMain/c/jni_glue.c",
            "settings.gradle.kts",
            "build.gradle.kts",
            "src/commonMain/kotlin/com/example/boltffi/BoltFFI.kt",
            KMP_SUPPORT_REPORT_FILE,
            "src/jvmMain/kotlin/com/example/boltffi/BoltFFIJvmActual.kt",
            "src/androidMain/kotlin/com/example/boltffi/BoltFFIAndroidActual.kt",
            "src/jvmMain/kotlin/com/example/boltffi/jvm/BoltFFI.kt",
            "src/androidMain/kotlin/com/example/boltffi/jvm/BoltFFI.kt",
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
    fn kmp_target_renders_sync_primitive_function_through_shared_jni_bridge() {
        let output = KmpHost::new()
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("classified primitive sync function should render");

        let common = file(
            &output,
            "src/commonMain/kotlin/com/example/boltffi/BoltFFI.kt",
        );
        assert!(common.contains("expect fun add(left: Int, right: Int): Int"));

        for path in [
            "src/jvmMain/kotlin/com/example/boltffi/BoltFFIJvmActual.kt",
            "src/androidMain/kotlin/com/example/boltffi/BoltFFIAndroidActual.kt",
        ] {
            let actual = file(&output, path);
            assert!(actual.contains("actual fun add(left: Int, right: Int): Int"));
            assert!(actual.contains("return com.example.boltffi.jvm.add(left, right)"));
            assert!(!actual.contains("FfiException"));
        }

        let internal = file(
            &output,
            "src/jvmMain/kotlin/com/example/boltffi/jvm/BoltFFI.kt",
        );
        assert!(internal.contains("external fun boltffi_function_demo_add"));
        assert!(internal.contains("return Native.boltffi_function_demo_add(left, right)"));

        let jni = file(&output, "src/jvmMain/c/jni_glue.c");
        assert!(jni.contains("Java_com_example_boltffi_jvm_Native_boltffi_1function_1demo_1add"));
        assert!(jni.contains("boltffi_function_demo_add(left, right)"));
        assert_eq!(jni, file(&output, "src/androidMain/c/jni_glue.c"));

        let report: serde_json::Value =
            serde_json::from_str(file(&output, KMP_SUPPORT_REPORT_FILE))
                .expect("valid support report");
        assert_eq!(report["admitted_apis"][0]["kind"], "function");
        assert_eq!(report["admitted_apis"][0]["name"], "add");
        assert_eq!(report["rejected_apis"], serde_json::json!([]));
    }

    #[test]
    fn kmp_target_derives_default_native_libraries_from_binding_package() {
        let output = KmpHost::new()
            .into_target()
            .render(&bindings_for_package(
                "sample-library",
                r#"
                #[export]
                pub fn value() -> i32 {
                    42
                }
                "#,
            ))
            .expect("package-derived native-library names should render");

        let internal = file(
            &output,
            "src/jvmMain/kotlin/com/example/boltffi/jvm/BoltFFI.kt",
        );
        assert!(internal.contains("val androidLibrary = \"sample-library\""));
        assert!(internal.contains("val desktopPreferredLibrary = \"sample_library_jni\""));
        assert!(internal.contains("val desktopFallbackLibrary = \"sample_library\""));
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

        assert_eq!(rejected_reasons.len(), 1, "{rejected_reasons:#?}");
        for reasons in [diagnostic_messages, unsupported_reasons, rejected_reasons] {
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
