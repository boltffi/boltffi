//! Output file and support metadata helpers for KMP emission.

use serde::{Deserialize, Serialize};

use super::super::plan::{KmpSupportApi, KmpSupportMode, KmpSupportReport};

/// Relative path of the generated KMP support metadata file.
pub const KMP_SUPPORT_REPORT_FILE: &str = "boltffi-kmp-support.json";

/// Schema version for `boltffi-kmp-support.json`.
pub const KMP_SUPPORT_REPORT_SCHEMA_VERSION: u32 = 1;

/// Directory under each KMP C source set that owns generated BoltFFI headers.
pub const KMP_GENERATED_C_HEADER_DIR: &str = "boltffi_generated";

/// JSON metadata written to [`KMP_SUPPORT_REPORT_FILE`] and verified by pack.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KmpSupportMetadata {
    /// Version of the JSON report schema.
    pub schema_version: u32,
    /// Effective support policy used when the module was generated.
    pub mode: KmpSupportMode,
    /// KMP platforms that the generated commonMain surface was checked against.
    pub selected_platforms: Vec<String>,
    /// Kotlin package used in generated common/platform sources.
    pub package_name: String,
    /// Kotlin source/module class name used in generated files.
    pub module_name: String,
    /// Android minimum SDK used in the generated Gradle module.
    pub min_sdk: u32,
    /// APIs emitted into the generated KMP module.
    pub admitted_apis: Vec<KmpSupportApiMetadata>,
    /// APIs rejected in strict mode or pruned in preview mode.
    pub rejected_apis: Vec<KmpSupportApiMetadata>,
    /// Version of `boltffi_backend` that generated the report.
    pub generator_version: String,
}

impl KmpSupportMetadata {
    pub(crate) fn new(
        report: &KmpSupportReport,
        package_name: &str,
        module_name: &str,
        min_sdk: u32,
    ) -> Self {
        Self {
            schema_version: KMP_SUPPORT_REPORT_SCHEMA_VERSION,
            mode: report.mode(),
            selected_platforms: report
                .selected_platforms()
                .iter()
                .map(|platform| platform.label())
                .map(str::to_owned)
                .collect(),
            package_name: package_name.to_string(),
            module_name: module_name.to_string(),
            min_sdk,
            admitted_apis: report
                .admitted_apis()
                .iter()
                .map(KmpSupportApiMetadata::from_api)
                .collect(),
            rejected_apis: report
                .rejected_apis()
                .iter()
                .map(KmpSupportApiMetadata::from_api)
                .collect(),
            generator_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// One API entry in generated KMP support metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KmpSupportApiMetadata {
    /// API kind, such as `function`, `record`, or `class`.
    pub kind: String,
    /// Stable display name for the API.
    pub name: String,
    /// Rejection or pruning reason. Admitted APIs leave this empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl KmpSupportApiMetadata {
    fn from_api(api: &KmpSupportApi) -> Self {
        Self {
            kind: api.kind().to_string(),
            name: api.name().to_string(),
            reason: api.reason().map(str::to_owned),
        }
    }
}
