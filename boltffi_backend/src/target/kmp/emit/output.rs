//! Output file and support metadata helpers for KMP emission.

use serde::Serialize;

use super::super::plan::{KmpSupportApi, KmpSupportMode, KmpSupportReport};

/// Relative path of the generated KMP support metadata file.
pub const KMP_SUPPORT_REPORT_FILE: &str = "boltffi-kmp-support.json";

/// Schema version for `boltffi-kmp-support.json`.
pub const KMP_SUPPORT_REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
pub(crate) struct KmpSupportMetadata<'report> {
    schema_version: u32,
    mode: &'static str,
    selected_platforms: Vec<&'static str>,
    package_name: &'report str,
    module_name: &'report str,
    min_sdk: u32,
    admitted_apis: Vec<KmpSupportApiMetadata<'report>>,
    rejected_apis: Vec<KmpSupportApiMetadata<'report>>,
    generator_version: &'static str,
}

impl<'report> KmpSupportMetadata<'report> {
    pub(crate) fn new(
        report: &'report KmpSupportReport,
        package_name: &'report str,
        module_name: &'report str,
        min_sdk: u32,
    ) -> Self {
        Self {
            schema_version: KMP_SUPPORT_REPORT_SCHEMA_VERSION,
            mode: mode_label(report.mode()),
            selected_platforms: report
                .selected_platforms()
                .iter()
                .map(|platform| platform.label())
                .collect(),
            package_name,
            module_name,
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
            generator_version: env!("CARGO_PKG_VERSION"),
        }
    }
}

#[derive(Serialize)]
struct KmpSupportApiMetadata<'api> {
    kind: &'static str,
    name: &'api str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'api str>,
}

impl<'api> KmpSupportApiMetadata<'api> {
    fn from_api(api: &'api KmpSupportApi) -> Self {
        Self {
            kind: api.kind(),
            name: api.name(),
            reason: api.reason(),
        }
    }
}

fn mode_label(mode: KmpSupportMode) -> &'static str {
    mode.label()
}
