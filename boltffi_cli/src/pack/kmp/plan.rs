use std::path::Path;

use crate::build::CargoBuildProfile;
use crate::cli::Result;
use crate::config::Config;
use crate::pack::java::plan::{JvmCargoContext, PreparedJvmPackaging};

use super::layout::KmpPackageLayout;

/// Inputs and derived paths needed to package a generated KMP module.
pub(crate) struct KmpPackagingPlan {
    layout: KmpPackageLayout,
    jvm_packaging: PreparedJvmPackaging,
}

impl KmpPackagingPlan {
    /// Builds a KMP packaging plan from the configured crate and prepared JVM matrix.
    pub(crate) fn new(config: &Config, jvm_packaging: PreparedJvmPackaging) -> Result<Self> {
        jvm_packaging.packaging_targets.first().ok_or_else(|| {
            crate::cli::CliError::CommandFailed {
                command: "could not resolve selected Cargo package for KMP packaging".to_string(),
                status: None,
            }
        })?;
        Ok(Self {
            layout: KmpPackageLayout::from_config(config),
            jvm_packaging,
        })
    }

    /// Returns the selected package manifest path used for metadata-backed generation.
    pub(crate) fn manifest_path(&self) -> &Path {
        self.cargo_context().library.manifest_path()
    }

    /// Returns the native artifact name selected from the JVM packaging matrix.
    pub(crate) fn artifact_name(&self) -> &str {
        self.cargo_context().library.artifact_name()
    }

    /// Returns the fallback C header basename for generated glue with no package include.
    pub(crate) fn fallback_header_name(&self) -> String {
        self.cargo_context()
            .library
            .package_name()
            .replace('-', "_")
    }

    /// Returns the rustup toolchain selector used for metadata-backed generation.
    pub(crate) fn generation_toolchain_selector(&self) -> Option<&str> {
        self.cargo_context().toolchain_selector.as_deref()
    }

    /// Returns the target directory reported by Cargo metadata for selected package builds.
    pub(crate) fn target_directory(&self) -> &Path {
        &self.cargo_context().target_directory
    }

    pub(crate) fn build_profile(&self) -> &CargoBuildProfile {
        &self.cargo_context().build_profile
    }

    /// Returns Cargo arguments used for metadata-backed generation.
    pub(crate) fn generation_cargo_args(&self, release: bool) -> Vec<String> {
        let mut args = self.cargo_context().cargo_command_args.as_slice().to_vec();
        if release && !cargo_args_select_profile(&args) {
            args.insert(0, "--release".to_string());
        }
        args
    }

    /// Returns the generated KMP project layout.
    pub(crate) fn layout(&self) -> &KmpPackageLayout {
        &self.layout
    }

    /// Returns the prepared JVM packaging matrix reused by KMP desktop packaging.
    pub(crate) fn jvm_packaging(&self) -> &PreparedJvmPackaging {
        &self.jvm_packaging
    }

    fn cargo_context(&self) -> &JvmCargoContext {
        &self.jvm_packaging.packaging_targets[0].cargo_context
    }
}

fn cargo_args_select_profile(cargo_args: &[String]) -> bool {
    cargo_args
        .iter()
        .any(|arg| arg == "--release" || arg == "--profile" || arg.starts_with("--profile="))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::build::CargoBuildProfile;
    use crate::cargo::SelectedLibrary;
    use crate::config::Config;
    use crate::pack::java::plan::{JvmCargoContext, JvmPackagingTarget, PreparedJvmPackaging};
    use crate::target::JavaHostTarget;
    use crate::toolchain::NativeHostToolchain;
    use boltffi_bindgen::cargo::LibraryCargoArgs;

    use super::KmpPackagingPlan;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn jvm_packaging() -> PreparedJvmPackaging {
        let current_host = JavaHostTarget::current().expect("current host");
        PreparedJvmPackaging {
            host_targets: vec![current_host],
            packaging_targets: vec![JvmPackagingTarget {
                cargo_context: JvmCargoContext {
                    host_target: current_host,
                    rust_target_triple: "x86_64-unknown-linux-gnu".to_string(),
                    release: true,
                    build_profile: CargoBuildProfile::Release,
                    library: SelectedLibrary::fixture(
                        "workspace-member",
                        "/tmp/workspace/member/Cargo.toml",
                        "workspace_member_ffi",
                    )
                    .fixture_cargo_manifest("/tmp/workspace/Cargo.toml"),
                    target_directory: PathBuf::from("/tmp/workspace/target"),
                    cargo_command_args: LibraryCargoArgs::parse([
                        "--features".to_string(),
                        "ffi".to_string(),
                    ])
                    .unwrap(),
                    toolchain_selector: Some("+nightly".to_string()),
                },
                toolchain: NativeHostToolchain::discover(
                    None,
                    &[],
                    Path::new("/tmp/workspace/Cargo.toml"),
                    current_host,
                    current_host,
                )
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

        let plan = KmpPackagingPlan::new(&config, jvm_packaging()).expect("KMP plan");

        assert_eq!(
            plan.manifest_path(),
            PathBuf::from("/tmp/workspace/member/Cargo.toml")
        );
        assert_eq!(plan.artifact_name(), "workspace_member_ffi");
        assert_eq!(plan.fallback_header_name(), "workspace_member");
        assert_eq!(plan.generation_toolchain_selector(), Some("+nightly"));
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
        assert_eq!(plan.build_profile(), &CargoBuildProfile::Release);
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
        let mut packaging = jvm_packaging();
        packaging.packaging_targets[0]
            .cargo_context
            .cargo_command_args =
            LibraryCargoArgs::parse(["--profile".to_string(), "dist".to_string()]).unwrap();

        let plan = KmpPackagingPlan::new(&config, packaging).expect("KMP plan");

        assert_eq!(
            plan.generation_cargo_args(true),
            vec!["--profile".to_string(), "dist".to_string()]
        );
    }
}
