//! Kotlin Multiplatform target for the IR backend pipeline.
//!
//! The backend lowers admitted APIs into a typed KMP plan and emits the
//! JVM/Android project layout incrementally. APIs whose body emission has not
//! been ported remain fail-closed so generated `commonMain` never exposes a
//! public API without matching platform actuals.

mod bridge;
pub mod emit;
mod host;
pub mod lower;
mod names;
pub mod plan;
mod syntax;

pub use bridge::{KmpBridge, KmpBridgeContract};
pub use emit::{
    KMP_GENERATED_C_HEADER_DIR, KMP_SUPPORT_REPORT_FILE, KMP_SUPPORT_REPORT_SCHEMA_VERSION,
    KmpEmissionOptions, KmpEmitter, KmpSupportApiMetadata, KmpSupportMetadata,
};
pub use host::{DEFAULT_KMP_MODULE_NAME, DEFAULT_KMP_PACKAGE_NAME, KmpHost};
pub use lower::{KmpLowerError, KmpLowerer, KmpLoweringOptions};
pub use plan::{
    KmpApiBody, KmpApiPlan, KmpCapability, KmpCapabilitySet, KmpCommonModule, KmpFunctionPlan,
    KmpJvmDelegateFunction, KmpJvmDelegateOutput, KmpModule, KmpParamPlan, KmpPlatform,
    KmpPlatformModule, KmpSupportApi, KmpSupportMode, KmpSupportReport, KmpTypePlan,
};
pub use syntax::Syntax;
