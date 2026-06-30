use super::layout::RubyStagingLayout;
use super::plan::RubyPackagingPlan;
use super::platform::RubyPackTarget;
use crate::cli::{CliError, Result};
use crate::pack::java::link::extract_native_static_libraries;
use crate::reporter::Step;
use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BindingMetadataSurface,
};
use std::process::Command;

pub struct RubyArchiveBuilder<'a> {
    plan: &'a RubyPackagingPlan,
    layout: &'a RubyStagingLayout,
}

impl<'a> RubyArchiveBuilder<'a> {
    pub fn new(plan: &'a RubyPackagingPlan, layout: &'a RubyStagingLayout) -> Self {
        Self { plan, layout }
    }

    pub fn verify_existing(&self, _target: &RubyPackTarget, _step: &Step) -> Result<()> {
        let expected_a_path = self
            .layout
            .ext_dir(self.plan)
            .join(format!("lib{}_ffi.a", self.plan.crate_name));
        let expected_libs_path = self
            .layout
            .ext_dir(self.plan)
            .join(format!("lib{}_ffi.a.native-libs", self.plan.crate_name));

        if !expected_a_path.exists() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "no-build mode specified, but missing precompiled archive: {}",
                    expected_a_path.display()
                ),
                status: None,
            });
        }
        if !expected_libs_path.exists() {
            return Err(CliError::CommandFailed {
                command: format!(
                    "no-build mode specified, but missing native-libs sidecar: {}",
                    expected_libs_path.display()
                ),
                status: None,
            });
        }

        Ok(())
    }

    pub fn build(&self, target: &RubyPackTarget, _step: &Step) -> Result<()> {
        let target_arg = target.zigbuild_target.clone();
        let manifest_path = &self.plan.cargo_manifest_path;

        let mut configured_command = self
            .plan
            .cargo_zigbuild
            .as_deref()
            .unwrap_or("cargo zigbuild")
            .split_whitespace();
        let program = configured_command
            .next()
            .ok_or_else(|| CliError::CommandFailed {
                command: "targets.ruby.pack.cargo_zigbuild must not be empty".to_string(),
                status: None,
            })?;
        let mut cmd = Command::new(program);
        if program == "cargo"
            && let Some(toolchain_selector) = self.plan.toolchain_selector.as_deref()
        {
            cmd.arg(toolchain_selector);
        }
        cmd.args(configured_command);
        cmd.args(["--manifest-path", &manifest_path.to_string_lossy(), "--lib"]);
        if let Some(package_selector) = self.plan.package_selector.as_deref() {
            cmd.arg("-p").arg(package_selector);
        }
        cmd.args(["--target", &target_arg]);
        cmd.args(["--message-format", "json-render-diagnostics"]);

        if self.plan.release {
            cmd.arg("--release");
        }
        cmd.args(&self.plan.cargo_command_args);
        Self::apply_ir_expansion_env(
            &mut cmd,
            &self.plan.cargo_manifest_path,
            &self.plan.library_source_path,
        )?;

        let command_label = format!("{:?}", cmd);
        let output = cmd.output().map_err(|e| CliError::CommandFailed {
            command: format!("Failed to invoke {command_label}: {e}"),
            status: None,
        })?;

        if !output.status.success() {
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::CommandFailed {
                command: format!("cargo zigbuild failed with: {stderr_str}"),
                status: output.status.code(),
            });
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let mut built_archive_path = None;

        for line in stdout_str.lines() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(reason) = val.get("reason").and_then(|r| r.as_str())
                && reason == "compiler-artifact"
                && let Some(target_val) = val.get("target")
                && let Some(kinds) = target_val.get("kind").and_then(|k| k.as_array())
                && kinds.iter().any(|k| k.as_str() == Some("staticlib"))
                && let Some(filenames) = val.get("filenames").and_then(|f| f.as_array())
            {
                for filename in filenames {
                    if let Some(fname_str) = filename.as_str()
                        && fname_str.ends_with(".a")
                    {
                        built_archive_path = Some(std::path::PathBuf::from(fname_str));
                    }
                }
            }
        }

        let archive_path = built_archive_path.ok_or_else(|| CliError::CommandFailed {
            command: "cargo zigbuild succeeded but no .a staticlib artifact was produced"
                .to_string(),
            status: None,
        })?;

        let ext_dir = self.layout.ext_dir(self.plan);
        std::fs::create_dir_all(&ext_dir).map_err(|e| CliError::CreateDirectoryFailed {
            path: ext_dir.clone(),
            source: e,
        })?;

        let dest_archive_name = format!("lib{}_ffi.a", self.plan.crate_name);
        let dest_archive_path = ext_dir.join(&dest_archive_name);
        std::fs::copy(&archive_path, &dest_archive_path).map_err(|e| CliError::CopyFailed {
            from: archive_path.clone(),
            to: dest_archive_path.clone(),
            source: e,
        })?;

        let mut rustc_cmd = Command::new("cargo");
        if let Some(toolchain_selector) = self.plan.toolchain_selector.as_deref() {
            rustc_cmd.arg(toolchain_selector);
        }
        rustc_cmd.args([
            "rustc",
            "--manifest-path",
            &manifest_path.to_string_lossy(),
            "--lib",
            "--target",
            target.rust_target,
        ]);
        if let Some(package_selector) = self.plan.package_selector.as_deref() {
            rustc_cmd.arg("-p").arg(package_selector);
        }
        if self.plan.release {
            rustc_cmd.arg("--release");
        }
        rustc_cmd.args(&self.plan.cargo_command_args);
        Self::apply_ir_expansion_env(
            &mut rustc_cmd,
            &self.plan.cargo_manifest_path,
            &self.plan.library_source_path,
        )?;
        rustc_cmd.args(["--", "--print", "native-static-libs"]);

        let mut libs_content = String::new();
        if let Ok(rustc_output) = rustc_cmd.output() {
            let stderr_str = String::from_utf8_lossy(&rustc_output.stderr);
            // Reuse the shared parser (ANSI-strip + `native-static-libs:` split)
            // so this matches the generated `extconf.rb` runtime path instead of
            // a brittle bare-prefix match.
            if let Some(libs) = extract_native_static_libraries(&stderr_str) {
                libs_content = libs.join(" ");
            }
        }

        let dest_libs_path = ext_dir.join(format!("lib{}_ffi.a.native-libs", self.plan.crate_name));
        std::fs::write(&dest_libs_path, &libs_content).map_err(|e| CliError::CommandFailed {
            command: format!("failed to write native-libs file: {e}"),
            status: None,
        })?;

        Ok(())
    }

    fn apply_ir_expansion_env(
        command: &mut Command,
        cargo_manifest_path: &std::path::Path,
        library_source_path: &std::path::Path,
    ) -> Result<()> {
        let root = cargo_manifest_path
            .parent()
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "manifest path '{}' has no parent directory",
                    cargo_manifest_path.display()
                ),
                status: None,
            })?;
        command.env(BINDING_EXPANSION_BUILD_ENV, "1");
        command.env(BINDING_EXPANSION_ROOT_ENV, root);
        command.env(BINDING_EXPANSION_SOURCE_ENV, library_source_path);
        command.env(
            BINDING_EXPANSION_SURFACE_ENV,
            BindingMetadataSurface::Native.as_str(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::process::Command;

    use boltffi_binding::{
        BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
        BINDING_EXPANSION_SURFACE_ENV,
    };

    use super::RubyArchiveBuilder;

    #[test]
    fn ruby_archive_build_command_sets_ir_expansion_environment() {
        let manifest = Path::new("/tmp/demo/Cargo.toml");
        let source = Path::new("/tmp/demo/src/lib.rs");
        let mut command = Command::new("cargo");

        RubyArchiveBuilder::apply_ir_expansion_env(&mut command, manifest, source)
            .expect("apply env");

        let env = command.get_envs().collect::<Vec<_>>();
        assert!(env.contains(&(
            OsStr::new(BINDING_EXPANSION_BUILD_ENV),
            Some(OsStr::new("1"))
        )));
        assert!(env.contains(&(
            OsStr::new(BINDING_EXPANSION_ROOT_ENV),
            Some(OsStr::new("/tmp/demo"))
        )));
        assert!(env.contains(&(
            OsStr::new(BINDING_EXPANSION_SOURCE_ENV),
            Some(OsStr::new("/tmp/demo/src/lib.rs"))
        )));
        assert!(env.contains(&(
            OsStr::new(BINDING_EXPANSION_SURFACE_ENV),
            Some(OsStr::new("native"))
        )));
    }
}
