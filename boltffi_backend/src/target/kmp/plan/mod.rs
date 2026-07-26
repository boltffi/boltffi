//! Typed Kotlin Multiplatform generation plan.

use std::collections::BTreeSet;

use boltffi_binding::Primitive;
use serde::{Deserialize, Serialize};

/// Feature required by one generated KMP API.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum KmpCapability {
    /// Direct record declarations.
    DirectRecords,
    /// Encoded record declarations.
    EncodedRecords,
    /// C-style enum declarations.
    CStyleEnums,
    /// Payload-carrying enum declarations.
    DataEnums,
    /// Synchronous exported callables.
    SyncCallables,
    /// Asynchronous exported callables.
    AsyncCallables,
    /// Mutating receiver callables, such as Rust `&mut self` methods.
    MutatingReceivers,
    /// Class handle declarations.
    Classes,
    /// Callback trait declarations.
    Callbacks,
    /// Stream declarations.
    Streams,
    /// Constant declarations.
    Constants,
    /// Custom type declarations.
    CustomTypes,
    /// Future binding shapes this backend does not know how to admit yet.
    UnknownBindingShapes,
}

impl KmpCapability {
    /// Returns a stable diagnostic label for this capability.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DirectRecords => "direct records",
            Self::EncodedRecords => "encoded records",
            Self::CStyleEnums => "c-style enums",
            Self::DataEnums => "data enums",
            Self::SyncCallables => "synchronous callables",
            Self::AsyncCallables => "asynchronous callables",
            Self::MutatingReceivers => "mutating receivers",
            Self::Classes => "classes",
            Self::Callbacks => "callbacks",
            Self::Streams => "streams",
            Self::Constants => "constants",
            Self::CustomTypes => "custom types",
            Self::UnknownBindingShapes => "unknown binding shapes",
        }
    }
}

/// Set of KMP capabilities supported by a platform or required by an API.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpCapabilitySet {
    capabilities: BTreeSet<KmpCapability>,
}

impl KmpCapabilitySet {
    /// Creates an empty capability set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the set contains the capability.
    pub fn contains(&self, capability: KmpCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Iterates capabilities in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = KmpCapability> + '_ {
        self.capabilities.iter().copied()
    }

    /// Returns whether this set contains every capability in `required`.
    pub fn supports_all(&self, required: &Self) -> bool {
        required.iter().all(|capability| self.contains(capability))
    }
}

impl FromIterator<KmpCapability> for KmpCapabilitySet {
    fn from_iter<T: IntoIterator<Item = KmpCapability>>(capabilities: T) -> Self {
        Self {
            capabilities: capabilities.into_iter().collect(),
        }
    }
}

/// KMP platform selected for the generated module.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum KmpPlatform {
    /// Kotlin/JVM target.
    Jvm,
    /// Kotlin Android target.
    Android,
    /// Kotlin/Native macOS arm64 target.
    MacosArm64,
    /// Kotlin/Native iOS simulator arm64 target.
    IosSimulatorArm64,
}

impl KmpPlatform {
    /// Returns the production platforms currently owned by the IR KMP path.
    pub fn default_selected() -> Vec<Self> {
        vec![Self::Jvm, Self::Android]
    }

    /// Returns the stable platform label written into reports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Jvm => "jvm",
            Self::Android => "android",
            Self::MacosArm64 => "macosArm64",
            Self::IosSimulatorArm64 => "iosSimulatorArm64",
        }
    }

    /// Returns the KMP capabilities this platform can satisfy today.
    pub fn capabilities(self) -> KmpCapabilitySet {
        match self {
            Self::Jvm | Self::Android => KmpCapabilitySet::from_iter([
                KmpCapability::DirectRecords,
                KmpCapability::EncodedRecords,
                KmpCapability::CStyleEnums,
                KmpCapability::DataEnums,
                KmpCapability::SyncCallables,
                KmpCapability::Constants,
                KmpCapability::CustomTypes,
            ]),
            Self::MacosArm64 | Self::IosSimulatorArm64 => KmpCapabilitySet::new(),
        }
    }
}

/// Effective support mode used while planning the KMP module.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum KmpSupportMode {
    /// Unsupported APIs fail planning.
    #[default]
    Strict,
    /// Unsupported APIs are omitted from the planned common surface.
    PreviewPruneUnsupported,
}

impl KmpSupportMode {
    /// Returns the metadata label for this mode.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::PreviewPruneUnsupported => "preview_prune_unsupported",
        }
    }
}

/// Complete planned KMP module before string rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpModule {
    common: KmpCommonModule,
    platforms: Vec<KmpPlatformModule>,
    support_report: KmpSupportReport,
    jvm_delegate: Option<KmpJvmDelegateOutput>,
}

impl KmpModule {
    /// Creates a KMP module plan.
    pub fn new(
        common: KmpCommonModule,
        platforms: Vec<KmpPlatformModule>,
        support_report: KmpSupportReport,
    ) -> Self {
        Self {
            common,
            platforms,
            support_report,
            jvm_delegate: None,
        }
    }

    /// Attaches JVM-family delegate output used by platform source-set emission.
    pub fn with_jvm_delegate(mut self, delegate: KmpJvmDelegateOutput) -> Self {
        self.jvm_delegate = Some(delegate);
        self
    }

    /// Returns the planned common source-set API surface.
    pub const fn common(&self) -> &KmpCommonModule {
        &self.common
    }

    /// Returns the selected platform modules.
    pub fn platforms(&self) -> &[KmpPlatformModule] {
        &self.platforms
    }

    /// Returns the support report for this plan.
    pub const fn support_report(&self) -> &KmpSupportReport {
        &self.support_report
    }

    /// Returns delegated JVM-family output, if this plan admitted delegated APIs.
    pub const fn jvm_delegate(&self) -> Option<&KmpJvmDelegateOutput> {
        self.jvm_delegate.as_ref()
    }
}

/// JVM-family output supplied by the Kotlin/JNI renderer for KMP source sets.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpJvmDelegateOutput {
    internal_package: String,
    internal_kotlin_runtime_source: String,
    shared_jni_source: String,
    functions: Vec<KmpJvmDelegateFunction>,
}

impl KmpJvmDelegateOutput {
    /// Creates delegate output for the functions rendered by the Kotlin/JNI backend.
    pub fn new(
        internal_package: impl Into<String>,
        internal_kotlin_runtime_source: impl Into<String>,
        functions: Vec<KmpJvmDelegateFunction>,
    ) -> Self {
        Self {
            internal_package: internal_package.into(),
            internal_kotlin_runtime_source: internal_kotlin_runtime_source.into(),
            shared_jni_source: String::new(),
            functions,
        }
    }

    /// Attaches shared JNI translation-unit source emitted before per-function glue.
    pub fn with_shared_jni_source(mut self, source: impl Into<String>) -> Self {
        self.shared_jni_source = source.into();
        self
    }

    /// Returns the Kotlin package declared by delegated JVM-family source.
    pub fn internal_package(&self) -> &str {
        &self.internal_package
    }

    /// Returns shared internal Kotlin runtime source used by delegated functions.
    pub fn internal_kotlin_runtime_source(&self) -> &str {
        &self.internal_kotlin_runtime_source
    }

    /// Returns shared JNI source used by every delegated JVM-family function.
    pub fn shared_jni_source(&self) -> &str {
        &self.shared_jni_source
    }

    /// Returns whether this delegate contains platform implementation for `function`.
    pub fn covers_function(&self, function: &KmpFunctionPlan) -> bool {
        self.function_for(function).is_some()
    }

    /// Returns delegated source with structural coverage for `function`, if present.
    pub fn function_for(&self, function: &KmpFunctionPlan) -> Option<&KmpJvmDelegateFunction> {
        self.functions
            .iter()
            .find(|candidate| candidate.matches(function))
    }
}

/// One function covered by a JVM-family delegate output.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpJvmDelegateFunction {
    native_symbol: String,
    kotlin_name: String,
    param_types: Vec<KmpTypePlan>,
    returns: Option<KmpTypePlan>,
    internal_kotlin_source: Option<String>,
    jni_glue_source: String,
}

impl KmpJvmDelegateFunction {
    /// Creates a delegated function signature for one native symbol.
    pub fn new(
        native_symbol: impl Into<String>,
        kotlin_name: impl Into<String>,
        param_types: Vec<KmpTypePlan>,
        returns: Option<KmpTypePlan>,
        jni_glue_source: impl Into<String>,
    ) -> Self {
        Self {
            native_symbol: native_symbol.into(),
            kotlin_name: kotlin_name.into(),
            param_types,
            returns,
            internal_kotlin_source: None,
            jni_glue_source: jni_glue_source.into(),
        }
    }

    /// Attaches trusted internal Kotlin function source emitted by the delegate owner.
    ///
    /// The backend treats this as generated delegate output, not arbitrary Kotlin input. Coverage
    /// admission remains based on the typed delegate signature and JNI glue.
    pub fn with_internal_kotlin_source(mut self, source: impl Into<String>) -> Self {
        self.internal_kotlin_source = Some(source.into());
        self
    }

    /// Returns the internal Kotlin function source emitted by the delegate owner.
    pub fn internal_kotlin_source(&self) -> Option<&str> {
        self.internal_kotlin_source.as_deref()
    }

    /// Returns the JNI C glue source snippet for this delegated function.
    pub fn jni_glue_source(&self) -> &str {
        &self.jni_glue_source
    }

    fn matches(&self, function: &KmpFunctionPlan) -> bool {
        self.native_symbol == function.native_symbol
            && self.kotlin_name == function.name
            && !self.jni_glue_source.trim().is_empty()
            && self.param_types.len() == function.params.len()
            && self
                .param_types
                .iter()
                .zip(function.params.iter())
                .all(|(expected, actual)| expected == actual.ty())
            && self.returns.as_ref() == function.returns.as_ref()
    }
}

/// Planned declarations emitted to `commonMain`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpCommonModule {
    apis: Vec<KmpApiPlan>,
}

impl KmpCommonModule {
    /// Creates a common module plan from admitted APIs.
    pub fn new(apis: Vec<KmpApiPlan>) -> Self {
        Self { apis }
    }

    /// Returns APIs admitted to `commonMain`.
    pub fn apis(&self) -> &[KmpApiPlan] {
        &self.apis
    }
}

/// Planned platform source-set module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpPlatformModule {
    platform: KmpPlatform,
    capabilities: KmpCapabilitySet,
}

impl KmpPlatformModule {
    /// Creates a platform module plan for one selected KMP platform.
    pub fn new(platform: KmpPlatform, capabilities: KmpCapabilitySet) -> Self {
        Self {
            platform,
            capabilities,
        }
    }

    /// Returns the selected platform.
    pub const fn platform(&self) -> KmpPlatform {
        self.platform
    }

    /// Returns capabilities advertised by the platform.
    pub const fn capabilities(&self) -> &KmpCapabilitySet {
        &self.capabilities
    }
}

/// One API admitted into the planned common surface.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpApiPlan {
    kind: &'static str,
    name: String,
    required_capabilities: KmpCapabilitySet,
    body: KmpApiBody,
}

impl KmpApiPlan {
    /// Creates an admitted API plan whose body emission has not been ported yet.
    pub fn new(
        kind: &'static str,
        name: impl Into<String>,
        required_capabilities: KmpCapabilitySet,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            required_capabilities,
            body: KmpApiBody::Unsupported,
        }
    }

    /// Creates an admitted function API plan.
    pub fn function(
        name: impl Into<String>,
        required_capabilities: KmpCapabilitySet,
        function: KmpFunctionPlan,
    ) -> Self {
        Self {
            kind: "function",
            name: name.into(),
            required_capabilities,
            body: KmpApiBody::Function(function),
        }
    }

    /// Returns the API kind, such as `function` or `record`.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Returns the stable API display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns capabilities required by this API.
    pub const fn required_capabilities(&self) -> &KmpCapabilitySet {
        &self.required_capabilities
    }

    /// Returns the renderable API body, if this API has been ported.
    pub const fn body(&self) -> &KmpApiBody {
        &self.body
    }
}

/// Renderable body for one admitted KMP API.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum KmpApiBody {
    /// Body emission for this admitted API shape has not been ported yet.
    Unsupported,
    /// Synchronous free function body.
    Function(KmpFunctionPlan),
}

/// Planned Kotlin function emitted to common and platform source sets.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpFunctionPlan {
    name: String,
    native_symbol: String,
    params: Vec<KmpParamPlan>,
    returns: Option<KmpTypePlan>,
}

impl KmpFunctionPlan {
    /// Creates a function plan.
    pub fn new(
        name: impl Into<String>,
        native_symbol: impl Into<String>,
        params: Vec<KmpParamPlan>,
        returns: Option<KmpTypePlan>,
    ) -> Self {
        Self {
            name: name.into(),
            native_symbol: native_symbol.into(),
            params,
            returns,
        }
    }

    /// Returns the generated Kotlin function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the native symbol called by the internal JVM binding.
    pub fn native_symbol(&self) -> &str {
        &self.native_symbol
    }

    /// Returns the generated Kotlin parameters.
    pub fn params(&self) -> &[KmpParamPlan] {
        &self.params
    }

    /// Returns the generated Kotlin return type, or `None` for `Unit`.
    pub const fn returns(&self) -> Option<&KmpTypePlan> {
        self.returns.as_ref()
    }
}

/// Planned Kotlin function parameter.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpParamPlan {
    name: String,
    ty: KmpTypePlan,
}

impl KmpParamPlan {
    /// Creates a parameter plan.
    pub fn new(name: impl Into<String>, ty: KmpTypePlan) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }

    /// Returns the generated Kotlin parameter name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the generated Kotlin parameter type.
    pub const fn ty(&self) -> &KmpTypePlan {
        &self.ty
    }
}

/// Planned Kotlin type for supported KMP declarations.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum KmpTypePlan {
    /// Primitive scalar type.
    Primitive(Primitive),
}

/// Generated support report for a planned KMP module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpSupportReport {
    mode: KmpSupportMode,
    selected_platforms: Vec<KmpPlatform>,
    admitted_apis: Vec<KmpSupportApi>,
    rejected_apis: Vec<KmpSupportApi>,
}

impl KmpSupportReport {
    /// Creates a KMP support report.
    pub fn new(
        mode: KmpSupportMode,
        selected_platforms: Vec<KmpPlatform>,
        admitted_apis: Vec<KmpSupportApi>,
        rejected_apis: Vec<KmpSupportApi>,
    ) -> Self {
        Self {
            mode,
            selected_platforms,
            admitted_apis,
            rejected_apis,
        }
    }

    /// Returns the effective support mode.
    pub const fn mode(&self) -> KmpSupportMode {
        self.mode
    }

    /// Returns the selected platform matrix.
    pub fn selected_platforms(&self) -> &[KmpPlatform] {
        &self.selected_platforms
    }

    /// Returns APIs admitted to the planned common surface.
    pub fn admitted_apis(&self) -> &[KmpSupportApi] {
        &self.admitted_apis
    }

    /// Returns APIs rejected in strict mode or pruned in preview mode.
    pub fn rejected_apis(&self) -> &[KmpSupportApi] {
        &self.rejected_apis
    }
}

/// One support report entry for an admitted or rejected API.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpSupportApi {
    kind: &'static str,
    name: String,
    reason: Option<String>,
}

impl KmpSupportApi {
    /// Creates an admitted support report entry.
    pub fn admitted(kind: &'static str, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            reason: None,
        }
    }

    /// Creates a rejected support report entry.
    pub fn rejected(
        kind: &'static str,
        name: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            reason: Some(reason.into()),
        }
    }

    /// Returns the API kind.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Returns the stable API display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the rejection reason, if this API was rejected.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}
