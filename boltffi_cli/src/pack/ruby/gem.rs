use super::layout::RubyStagingLayout;
use super::plan::RubyPackagingPlan;
use super::platform::RubyPackTarget;
use crate::cli::{CliError, Result};
use crate::reporter::Step;
use std::path::PathBuf;
use std::process::Command;

pub struct RubyGemBuilder<'a> {
    plan: &'a RubyPackagingPlan,
    layout: &'a RubyStagingLayout,
}

impl<'a> RubyGemBuilder<'a> {
    pub fn new(plan: &'a RubyPackagingPlan, layout: &'a RubyStagingLayout) -> Self {
        Self { plan, layout }
    }

    fn source_gemspec_path(&self) -> Result<PathBuf> {
        let mut candidates = Vec::new();
        candidates.push(
            self.layout
                .source_root
                .join(format!("{}.gemspec", self.plan.gem_name)),
        );

        let crate_gemspec = self
            .layout
            .source_root
            .join(format!("{}.gemspec", self.plan.crate_name));
        if !candidates
            .iter()
            .any(|candidate| candidate == &crate_gemspec)
        {
            candidates.push(crate_gemspec);
        }

        candidates
            .iter()
            .find(|candidate| candidate.exists())
            .cloned()
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "source gemspec not found; tried {}",
                    candidates
                        .iter()
                        .map(|candidate| candidate.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                status: None,
            })
    }

    pub fn build_gem(&self, target: &RubyPackTarget, _step: &Step) -> Result<PathBuf> {
        let source_gemspec_path = self.source_gemspec_path()?;

        let original_gemspec =
            std::fs::read_to_string(&source_gemspec_path).map_err(|e| CliError::ReadFailed {
                path: source_gemspec_path.clone(),
                source: e,
            })?;

        let mut new_lines = Vec::new();
        let mut skip_mode = false;
        for line in original_gemspec.lines() {
            if line.contains("spec.platform = Gem::Platform::RUBY") {
                new_lines.push(format!(
                    "  spec.platform = Gem::Platform.new(\"{}\")",
                    target.rubygems_platform
                ));
                continue;
            }
            if line.contains("spec.files =") {
                new_lines.push("  spec.files = [".to_string());
                new_lines.push(format!("    \"lib/{}.rb\",", self.plan.crate_name));
                new_lines.push(format!("    \"ext/{}/_native.c\",", self.plan.crate_name));
                new_lines.push(format!("    \"ext/{}/boltffi.h\",", self.plan.crate_name));
                new_lines.push(format!("    \"ext/{}/extconf.rb\",", self.plan.crate_name));
                new_lines.push(format!(
                    "    \"ext/{}/lib{}_ffi.a\",",
                    self.plan.crate_name, self.plan.crate_name
                ));
                new_lines.push(format!(
                    "    \"ext/{}/lib{}_ffi.a.native-libs\",",
                    self.plan.crate_name, self.plan.crate_name
                ));
                new_lines.push("  ]".to_string());
                skip_mode = true;
                continue;
            }
            if skip_mode {
                if line.contains("]") || line.contains("File.directory?(path)") {
                    skip_mode = false;
                }
                continue;
            }
            new_lines.push(line.to_string());
        }
        let precompiled_gemspec = new_lines.join("\n");

        let staging_dir = self.layout.target_staging_dir(self.plan, target);
        if staging_dir.exists() {
            std::fs::remove_dir_all(&staging_dir).ok();
        }
        std::fs::create_dir_all(&staging_dir).map_err(|e| CliError::CreateDirectoryFailed {
            path: staging_dir.clone(),
            source: e,
        })?;

        let rb_src = self
            .layout
            .source_root
            .join("lib")
            .join(format!("{}.rb", self.plan.crate_name));
        let rb_dest = staging_dir
            .join("lib")
            .join(format!("{}.rb", self.plan.crate_name));
        std::fs::create_dir_all(rb_dest.parent().unwrap()).ok();
        std::fs::copy(&rb_src, &rb_dest).map_err(|e| CliError::CopyFailed {
            from: rb_src,
            to: rb_dest,
            source: e,
        })?;

        let c_src = self.layout.ext_dir(self.plan).join("_native.c");
        let c_dest = staging_dir
            .join("ext")
            .join(&self.plan.crate_name)
            .join("_native.c");
        std::fs::create_dir_all(c_dest.parent().unwrap()).ok();
        std::fs::copy(&c_src, &c_dest).map_err(|e| CliError::CopyFailed {
            from: c_src,
            to: c_dest,
            source: e,
        })?;

        let header_src = self.layout.ext_dir(self.plan).join("boltffi.h");
        let header_dest = staging_dir
            .join("ext")
            .join(&self.plan.crate_name)
            .join("boltffi.h");
        std::fs::copy(&header_src, &header_dest).map_err(|e| CliError::CopyFailed {
            from: header_src,
            to: header_dest,
            source: e,
        })?;

        let ext_src = self.layout.ext_dir(self.plan).join("extconf.rb");
        let ext_dest = staging_dir
            .join("ext")
            .join(&self.plan.crate_name)
            .join("extconf.rb");
        std::fs::copy(&ext_src, &ext_dest).map_err(|e| CliError::CopyFailed {
            from: ext_src,
            to: ext_dest,
            source: e,
        })?;

        let a_src = self
            .layout
            .ext_dir(self.plan)
            .join(format!("lib{}_ffi.a", self.plan.crate_name));
        let a_dest = staging_dir
            .join("ext")
            .join(&self.plan.crate_name)
            .join(format!("lib{}_ffi.a", self.plan.crate_name));
        std::fs::copy(&a_src, &a_dest).map_err(|e| CliError::CopyFailed {
            from: a_src,
            to: a_dest,
            source: e,
        })?;

        let libs_src = self
            .layout
            .ext_dir(self.plan)
            .join(format!("lib{}_ffi.a.native-libs", self.plan.crate_name));
        let libs_dest = staging_dir
            .join("ext")
            .join(&self.plan.crate_name)
            .join(format!("lib{}_ffi.a.native-libs", self.plan.crate_name));
        std::fs::copy(&libs_src, &libs_dest).map_err(|e| CliError::CopyFailed {
            from: libs_src,
            to: libs_dest,
            source: e,
        })?;

        let gemspec_path = staging_dir.join(format!("{}.gemspec", self.plan.gem_name));
        std::fs::write(&gemspec_path, &precompiled_gemspec).map_err(|e| {
            CliError::CommandFailed {
                command: format!("failed to write precompiled gemspec: {e}"),
                status: None,
            }
        })?;

        std::fs::create_dir_all(&self.layout.target_root).map_err(|e| {
            CliError::CreateDirectoryFailed {
                path: self.layout.target_root.clone(),
                source: e,
            }
        })?;

        let mut gem_cmd = Command::new("gem");
        gem_cmd.args([
            "build",
            &format!("{}.gemspec", self.plan.gem_name),
            "--output",
            &self
                .layout
                .target_root
                .join(format!(
                    "{}-{}-{}.gem",
                    self.plan.gem_name, self.plan.version, target.rubygems_platform
                ))
                .to_string_lossy(),
        ]);
        gem_cmd.current_dir(&staging_dir);

        let gem_output = gem_cmd.output().map_err(|e| CliError::CommandFailed {
            command: format!("failed to invoke gem build: {e}"),
            status: None,
        })?;

        if !gem_output.status.success() {
            let stderr_str = String::from_utf8_lossy(&gem_output.stderr);
            return Err(CliError::CommandFailed {
                command: format!("gem build failed with: {stderr_str}"),
                status: gem_output.status.code(),
            });
        }

        let gem_file_path = self.layout.target_root.join(format!(
            "{}-{}-{}.gem",
            self.plan.gem_name, self.plan.version, target.rubygems_platform
        ));
        Ok(gem_file_path)
    }
}
