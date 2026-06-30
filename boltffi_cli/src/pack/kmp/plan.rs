use std::path::{Path, PathBuf};

use crate::cli::Result;
use crate::config::Config;
use crate::pack::java::plan::PreparedJvmPackaging;

use super::layout::KmpPackageLayout;

/// Inputs and derived paths needed to package a generated KMP module.
pub(crate) struct KmpPackagingPlan {
    cargo_manifest_path: PathBuf,
    manifest_path: PathBuf,
    package_name: String,
    package_selector: Option<String>,
    artifact_name: String,
    target_directory: PathBuf,
    cargo_command_args: Vec<String>,
    toolchain_selector: Option<String>,
    layout: KmpPackageLayout,
    jvm_packaging: PreparedJvmPackaging,
}

impl KmpPackagingPlan {
    /// Builds a KMP packaging plan from the configured crate and prepared JVM matrix.
    pub(crate) fn new(config: &Config, jvm_packaging: PreparedJvmPackaging) -> Result<Self> {
        let cargo_context = jvm_packaging
            .packaging_targets
            .first()
            .map(|target| &target.cargo_context)
            .ok_or_else(|| crate::cli::CliError::CommandFailed {
                command: "could not resolve selected Cargo package for KMP packaging".to_string(),
                status: None,
            })?;
        Ok(Self {
            cargo_manifest_path: cargo_context.cargo_manifest_path.clone(),
            manifest_path: cargo_context.manifest_path.clone(),
            package_name: cargo_context.package_name.clone(),
            package_selector: cargo_context.package_selector.clone(),
            artifact_name: cargo_context.artifact_name.clone(),
            target_directory: cargo_context.target_directory.clone(),
            cargo_command_args: cargo_context.cargo_command_args.clone(),
            toolchain_selector: cargo_context.toolchain_selector.clone(),
            layout: KmpPackageLayout::from_config(config),
            jvm_packaging,
        })
    }

    /// Returns the selected package manifest path used for metadata-backed generation.
    pub(crate) fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Returns the native artifact name selected from the JVM packaging matrix.
    pub(crate) fn artifact_name(&self) -> &str {
        &self.artifact_name
    }

    /// Returns the fallback C header basename for generated glue with no package include.
    pub(crate) fn fallback_header_name(&self) -> String {
        self.package_name.replace('-', "_")
    }

    /// Returns the rustup toolchain selector used for metadata-backed generation.
    pub(crate) fn generation_toolchain_selector(&self) -> Option<&str> {
        self.toolchain_selector.as_deref()
    }

    /// Returns the Cargo package selector to use for KMP-owned native builds.
    pub(crate) fn build_package_selector(&self) -> String {
        self.package_selector
            .clone()
            .unwrap_or_else(|| self.package_name.clone())
    }

    /// Returns the target directory reported by Cargo metadata for selected package builds.
    pub(crate) fn target_directory(&self) -> &Path {
        &self.target_directory
    }

    /// Returns Cargo arguments used for metadata-backed generation.
    pub(crate) fn generation_cargo_args(&self, release: bool) -> Vec<String> {
        let mut args = self.cargo_command_args.clone();
        if release && !cargo_args_select_profile(&args) {
            args.insert(0, "--release".to_string());
        }
        args
    }

    /// Returns Cargo arguments used for Android native builds owned by KMP packaging.
    pub(crate) fn android_build_cargo_args(&self) -> Vec<String> {
        self.toolchain_selector
            .iter()
            .cloned()
            .chain([
                "--manifest-path".to_string(),
                self.cargo_manifest_path.display().to_string(),
            ])
            .chain(self.cargo_command_args.iter().cloned())
            .collect()
    }

    /// Returns the generated KMP project layout.
    pub(crate) fn layout(&self) -> &KmpPackageLayout {
        &self.layout
    }

    /// Returns the prepared JVM packaging matrix reused by KMP desktop packaging.
    pub(crate) fn jvm_packaging(&self) -> &PreparedJvmPackaging {
        &self.jvm_packaging
    }
}

fn cargo_args_select_profile(cargo_args: &[String]) -> bool {
    cargo_args
        .iter()
        .any(|arg| arg == "--release" || arg == "--profile" || arg.starts_with("--profile="))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::build::CargoBuildProfile;
    use crate::config::Config;
    use crate::pack::java::plan::{
        JvmCargoContext, JvmCrateOutputs, JvmPackagingTarget, PreparedJvmPackaging,
    };
    use crate::target::JavaHostTarget;
    use crate::toolchain::NativeHostToolchain;

    use super::KmpPackagingPlan;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn jvm_packaging(package_selector: Option<String>) -> PreparedJvmPackaging {
        let current_host = JavaHostTarget::current().expect("current host");
        PreparedJvmPackaging {
            host_targets: vec![current_host],
            packaging_targets: vec![JvmPackagingTarget {
                cargo_context: JvmCargoContext {
                    host_target: current_host,
                    rust_target_triple: "x86_64-unknown-linux-gnu".to_string(),
                    release: true,
                    build_profile: CargoBuildProfile::Release,
                    package_name: "workspace-member".to_string(),
                    artifact_name: "workspace_member_ffi".to_string(),
                    cargo_manifest_path: PathBuf::from("/tmp/workspace/Cargo.toml"),
                    manifest_path: PathBuf::from("/tmp/workspace/member/Cargo.toml"),
                    package_selector,
                    target_directory: PathBuf::from("/tmp/workspace/target"),
                    cargo_command_args: vec!["--features".to_string(), "ffi".to_string()],
                    toolchain_selector: Some("+nightly".to_string()),
                    crate_outputs: JvmCrateOutputs {
                        builds_staticlib: true,
                        builds_cdylib: true,
                    },
                },
                toolchain: NativeHostToolchain::discover(None, &[], current_host, current_host)
                    .expect("native host toolchain"),
            }],
        }
    }

    #[test]
    fn kmp_packaging_plan_uses_selected_metadata_package_for_generation_and_builds() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "workspace-root"

[targets.kotlin_multiplatform]
enabled = true
output = "dist/kmp"
"#,
        );

        let plan =
            KmpPackagingPlan::new(&config, jvm_packaging(Some("workspace-member".to_string())))
                .expect("KMP plan");

        assert_eq!(
            plan.manifest_path(),
            PathBuf::from("/tmp/workspace/member/Cargo.toml")
        );
        assert_eq!(plan.artifact_name(), "workspace_member_ffi");
        assert_eq!(plan.fallback_header_name(), "workspace_member");
        assert_eq!(plan.generation_toolchain_selector(), Some("+nightly"));
        assert_eq!(plan.build_package_selector(), "workspace-member");
        assert_eq!(
            plan.target_directory(),
            PathBuf::from("/tmp/workspace/target")
        );
        assert_eq!(
            plan.generation_cargo_args(true),
            vec![
                "--release".to_string(),
                "--features".to_string(),
                "ffi".to_string(),
            ]
        );
        assert_eq!(
            plan.android_build_cargo_args(),
            vec![
                "+nightly".to_string(),
                "--manifest-path".to_string(),
                "/tmp/workspace/Cargo.toml".to_string(),
                "--features".to_string(),
                "ffi".to_string(),
            ]
        );
    }

    #[test]
    fn kmp_packaging_plan_does_not_duplicate_explicit_generation_profile() {
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "workspace-root"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );
        let mut packaging = jvm_packaging(None);
        packaging.packaging_targets[0]
            .cargo_context
            .cargo_command_args = vec!["--profile".to_string(), "dist".to_string()];

        let plan = KmpPackagingPlan::new(&config, packaging).expect("KMP plan");

        assert_eq!(
            plan.generation_cargo_args(true),
            vec!["--profile".to_string(), "dist".to_string()]
        );
        assert_eq!(plan.build_package_selector(), "workspace-member");
    }
}
