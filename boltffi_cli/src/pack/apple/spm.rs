use std::path::{Component, Path, PathBuf};

use askama::Template;

use crate::cli::{CliError, Result};
use crate::config::{Config, SpmLayout};

use super::names::AppleNames;
use super::write_generated_file;
use super::xcframework::render_library_modulemap;

pub struct SpmPackageGenerator<'a> {
    config: &'a Config,
    names: AppleNames,
    distribution: SpmPackageDistribution,
    layout: SpmLayout,
}

enum SpmPackageDistribution {
    Local,
    Remote(RemoteSpmPackage),
}

struct RemoteSpmPackage {
    checksum: String,
    version: String,
}

struct ApplePackageManifest {
    tools_version: String,
    package_name: String,
    product_target_name: String,
    binary_target_name: String,
    ffi_module_name: String,
    module_name: String,
    wrapper_sources: String,
    xcframework_name: String,
    platform_declarations: Vec<String>,
    has_wrapper_target: bool,
}

#[derive(Template)]
#[template(path = "ApplePackageLocal.swift", escape = "none")]
struct AppleLocalPackageTemplate<'a> {
    manifest: &'a ApplePackageManifest,
    xcframework_path: &'a str,
}

#[derive(Template)]
#[template(path = "ApplePackageRemote.swift", escape = "none")]
struct AppleRemotePackageTemplate<'a> {
    manifest: &'a ApplePackageManifest,
    repo_url: &'a str,
    version: &'a str,
    checksum: &'a str,
}

impl<'a> SpmPackageGenerator<'a> {
    pub fn new_local(config: &'a Config, layout: SpmLayout) -> Self {
        Self {
            config,
            names: AppleNames::from_config(config),
            distribution: SpmPackageDistribution::Local,
            layout,
        }
    }

    pub fn new_remote(
        config: &'a Config,
        checksum: String,
        version: String,
        layout: SpmLayout,
    ) -> Self {
        Self {
            config,
            names: AppleNames::from_config(config),
            distribution: SpmPackageDistribution::Remote(RemoteSpmPackage { checksum, version }),
            layout,
        }
    }

    pub fn generate(&self) -> Result<PathBuf> {
        let output_path = self.config.apple_spm_output().join("Package.swift");

        let content = self.render_package()?;

        let spm_output = self.config.apple_spm_output();
        std::fs::create_dir_all(&spm_output).map_err(|source| CliError::CreateDirectoryFailed {
            path: spm_output.clone(),
            source,
        })?;

        std::fs::write(&output_path, content).map_err(|source| CliError::WriteFailed {
            path: output_path.clone(),
            source,
        })?;

        self.generate_module_discovery()?;

        Ok(output_path)
    }

    fn generate_module_discovery(&self) -> Result<()> {
        // SwiftPM passes a C target's module map to downstream dependency scanners.
        // Keep this outside Sources so Swift wrapper targets never mix C and Swift.
        let discovery = self.config.apple_spm_output().join("FFI");
        write_generated_file(
            &discovery.join("include/module.modulemap"),
            &render_library_modulemap(self.names.ffi_module_name(), "ffi.h")?,
        )?;
        write_generated_file(
            &discovery.join("include/ffi.h"),
            &format!(
                "#pragma once\n#include <{0}/{0}.h>\n",
                self.names.library_name()
            ),
        )?;
        write_generated_file(
            &discovery.join("discovery.c"),
            "#include \"include/ffi.h\"\n",
        )
    }

    fn render_package(&self) -> Result<String> {
        match &self.distribution {
            SpmPackageDistribution::Local => self.render_local_package(),
            SpmPackageDistribution::Remote(remote_package) => {
                self.render_remote_package_with(remote_package)
            }
        }
    }

    fn render_local_package(&self) -> Result<String> {
        self.validate_wrapper_discovery_paths()?;
        let manifest = self.package_manifest();
        let xcframework_path = self.local_xcframework_path();

        render_apple_package_template(
            AppleLocalPackageTemplate {
                manifest: &manifest,
                xcframework_path: &xcframework_path,
            },
            "local",
        )
    }

    fn render_remote_package_with(&self, remote_package: &RemoteSpmPackage) -> Result<String> {
        self.validate_wrapper_discovery_paths()?;
        let manifest = self.package_manifest();
        let repo_url = self
            .config
            .apple_spm_repo_url()
            .unwrap_or("https://github.com/user/repo");

        render_apple_package_template(
            AppleRemotePackageTemplate {
                manifest: &manifest,
                repo_url,
                version: &remote_package.version,
                checksum: &remote_package.checksum,
            },
            "remote",
        )
    }

    fn package_manifest(&self) -> ApplePackageManifest {
        let layout = self.layout;
        let package_name = self.package_name_for_layout(layout);
        let module_name = self.names.swift_module_name().to_string();
        let tools_version = self
            .config
            .apple_swift_tools_version()
            .unwrap_or("5.9")
            .to_string();
        let wrapper_sources = self.wrapper_sources_path(layout);
        let ffi_module_name = self.names.ffi_module_name().to_string();
        let binary_target_name = ffi_binary_target_name(&ffi_module_name, &module_name);
        let product_target_name = if matches!(layout, SpmLayout::Split) {
            ffi_module_name.clone()
        } else {
            module_name.clone()
        };
        let platform_declarations = self.platform_declarations();

        ApplePackageManifest {
            tools_version,
            package_name,
            product_target_name,
            binary_target_name,
            ffi_module_name,
            module_name,
            wrapper_sources,
            xcframework_name: self.names.xcframework_name().to_string(),
            platform_declarations,
            has_wrapper_target: !matches!(layout, SpmLayout::Split),
        }
    }

    fn validate_wrapper_discovery_paths(&self) -> Result<()> {
        if !matches!(self.layout, SpmLayout::Split) {
            let package_root = self.config.apple_spm_output();
            let wrapper_path = PathBuf::from(self.wrapper_sources_path(self.layout));
            let (wrapper_path, discovery_path) = if wrapper_path.is_absolute() {
                let package_root = if package_root.is_absolute() {
                    package_root
                } else {
                    std::env::current_dir()
                        .map_err(|source| CliError::CommandFailed {
                            command: format!("resolve Apple Swift package directory: {source}"),
                            status: None,
                        })?
                        .join(package_root)
                };
                (
                    std::path::absolute(&wrapper_path).map_err(|source| {
                        CliError::CommandFailed {
                            command: format!("resolve Apple wrapper sources path: {source}"),
                            status: None,
                        }
                    })?,
                    std::path::absolute(package_root.join("FFI")).map_err(|source| {
                        CliError::CommandFailed {
                            command: format!("resolve Apple FFI discovery path: {source}"),
                            status: None,
                        }
                    })?,
                )
            } else {
                (
                    normalize_package_path(&wrapper_path.to_string_lossy()),
                    PathBuf::from("FFI"),
                )
            };
            if wrapper_path.starts_with(&discovery_path)
                || discovery_path.starts_with(&wrapper_path)
            {
                return Err(CliError::CommandFailed {
                    command: format!(
                        "Apple wrapper sources path {:?} overlaps generated FFI module discovery sources at {:?}",
                        self.wrapper_sources_path(self.layout),
                        discovery_path
                    ),
                    status: None,
                });
            }
        }
        Ok(())
    }

    fn ios_version_for_spm(&self) -> String {
        let deployment_target = self.config.apple_deployment_target();

        deployment_target
            .split('.')
            .next()
            .map(|major| format!("v{}", major))
            .unwrap_or_else(|| "v16".to_string())
    }

    fn platform_declarations(&self) -> Vec<String> {
        let mut platforms = Vec::new();

        if self.supports_ios_platform() {
            platforms.push(format!(".iOS(.{})", self.ios_version_for_spm()));
        }

        if self.supports_macos_platform() {
            platforms.push(".macOS(.v13)".to_string());
        }

        platforms
    }

    fn supports_ios_platform(&self) -> bool {
        !self.config.apple_ios_targets().is_empty()
            || !self.config.apple_simulator_targets().is_empty()
    }

    fn supports_macos_platform(&self) -> bool {
        self.config.apple_include_macos() && !self.config.apple_macos_targets().is_empty()
    }

    fn wrapper_sources_path(&self, layout: SpmLayout) -> String {
        match layout {
            SpmLayout::Bundled => self
                .config
                .apple_spm_wrapper_sources()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "Sources".to_string()),
            SpmLayout::Split | SpmLayout::FfiOnly => "Sources".to_string(),
        }
    }

    fn local_xcframework_path(&self) -> String {
        let package_root = self.config.apple_spm_output();
        let xcframework_path = self
            .config
            .apple_xcframework_output()
            .join(format!("{}.xcframework", self.names.xcframework_name()));
        let rel = relative_path(&package_root, &xcframework_path);
        rel.to_string_lossy().to_string()
    }

    fn package_name_for_layout(&self, layout: SpmLayout) -> String {
        self.config
            .apple_spm_package_name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| match layout {
                SpmLayout::Split => self.names.ffi_module_name().to_string(),
                SpmLayout::Bundled | SpmLayout::FfiOnly => {
                    self.names.swift_module_name().to_string()
                }
            })
    }
}

fn ffi_binary_target_name(ffi_module_name: &str, swift_module_name: &str) -> String {
    let mut name = format!("{ffi_module_name}Binary");
    while name == ffi_module_name || name == swift_module_name {
        name.push_str("Binary");
    }
    name
}

fn normalize_package_path(path: &str) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn render_apple_package_template(template: impl Template, distribution: &str) -> Result<String> {
    template.render().map_err(|source| CliError::CommandFailed {
        command: format!("render {distribution} Apple Package.swift template: {source}"),
        status: None,
    })
}

fn relative_path(from_dir: &std::path::Path, to_path: &std::path::Path) -> PathBuf {
    if from_dir == std::path::Path::new(".") || from_dir == std::path::Path::new("") {
        return to_path.to_path_buf();
    }

    let from_components = from_dir.components().collect::<Vec<_>>();
    let to_components = to_path.components().collect::<Vec<_>>();

    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let parent_count = from_components.len().saturating_sub(common_len);
    let parent_prefix = (0..parent_count).map(|_| std::path::Component::ParentDir);
    let suffix = to_components.iter().skip(common_len).copied();

    parent_prefix.chain(suffix).collect()
}

#[cfg(test)]
mod tests {
    use crate::config::{PackageConfig, SpmDistribution, TargetsConfig};
    use crate::target::Architecture;

    use super::*;

    struct AppleSpmConfigBuilder {
        config: Config,
    }

    impl AppleSpmConfigBuilder {
        fn new() -> Self {
            Self {
                config: Config {
                    experimental: Vec::new(),
                    cargo: Default::default(),
                    package: PackageConfig {
                        name: "mylib".to_string(),
                        crate_name: None,
                        version: None,
                        description: None,
                        license: None,
                        repository: None,
                    },
                    targets: TargetsConfig::default(),
                },
            }
        }

        fn with_macos_enabled(mut self) -> Self {
            self.config.targets.apple.include_macos = true;
            self
        }

        fn without_macos_slices(mut self) -> Self {
            self.config.targets.apple.macos_architectures = Some(Vec::new());
            self
        }

        fn without_ios_slices(mut self) -> Self {
            self.config.targets.apple.ios_architectures = Some(Vec::new());
            self.config.targets.apple.simulator_architectures = Some(Vec::new());
            self
        }

        fn with_macos_arm64_slice(mut self) -> Self {
            self.config.targets.apple.macos_architectures = Some(vec![Architecture::Arm64]);
            self
        }

        fn with_simulator_arm64_slice(mut self) -> Self {
            self.config.targets.apple.simulator_architectures = Some(vec![Architecture::Arm64]);
            self
        }

        fn with_remote_spm(mut self, repo_url: &str) -> Self {
            self.config.targets.apple.spm.distribution = SpmDistribution::Remote;
            self.config.targets.apple.spm.repo_url = Some(repo_url.to_string());
            self
        }

        fn with_module_name(mut self, module_name: &str) -> Self {
            self.config.targets.apple.swift.module_name = Some(module_name.to_string());
            self
        }

        fn with_ffi_module_name(mut self, module_name: &str) -> Self {
            self.config.targets.apple.swift.ffi_module_name = Some(module_name.to_string());
            self
        }

        fn with_xcframework_name(mut self, name: &str) -> Self {
            self.config.targets.apple.xcframework.name = Some(name.to_string());
            self
        }

        fn with_wrapper_sources(mut self, path: &str) -> Self {
            self.config.targets.apple.spm.wrapper_sources = Some(path.into());
            self
        }

        fn build(self) -> Config {
            self.config.validate().expect("config validation failed");
            self.config
        }
    }

    #[test]
    fn all_layouts_route_module_discovery_through_a_c_target() {
        let config = AppleSpmConfigBuilder::new().build();
        for layout in [SpmLayout::Bundled, SpmLayout::FfiOnly, SpmLayout::Split] {
            for generator in [
                SpmPackageGenerator::new_local(&config, layout),
                SpmPackageGenerator::new_remote(
                    &config,
                    "checksum".to_string(),
                    "1.0.0".to_string(),
                    layout,
                ),
            ] {
                let package = generator.render_package().expect("render package");
                assert!(package.contains("name: \"MylibFFIBinary\""));
                assert!(package.contains("name: \"MylibFFI\""));
                assert!(package.contains("dependencies: [\"MylibFFIBinary\"]"));
                assert!(package.contains("path: \"FFI\""));
                assert!(package.contains("publicHeadersPath: \"include\""));
                if matches!(layout, SpmLayout::Split) {
                    assert!(package.contains("targets: [\"MylibFFI\"]"));
                    assert!(!package.contains("path: \"Sources\""));
                } else {
                    assert!(package.contains("dependencies: [\"MylibFFI\"]"));
                }
                assert!(!package.contains("unsafeFlags"));
            }
        }
    }

    #[test]
    fn generates_discovery_sources_alongside_the_manifest() {
        let temporary_directory = tempfile::tempdir().expect("temporary package");
        let mut config = AppleSpmConfigBuilder::new().build();
        config.targets.apple.spm.output = Some(temporary_directory.path().to_path_buf());
        SpmPackageGenerator::new_local(&config, SpmLayout::Split)
            .generate()
            .expect("generate package");
        let discovery = temporary_directory.path().join("FFI");
        assert_eq!(
            std::fs::read_to_string(discovery.join("include/module.modulemap")).unwrap(),
            "module MylibFFI {\n    header \"ffi.h\"\n    export *\n}"
        );
        assert_eq!(
            std::fs::read_to_string(discovery.join("include/ffi.h")).unwrap(),
            "#pragma once\n#include <mylib/mylib.h>\n"
        );
        assert_eq!(
            std::fs::read_to_string(discovery.join("discovery.c")).unwrap(),
            "#include \"include/ffi.h\"\n"
        );
    }

    #[test]
    fn bundled_wrapper_sources_cannot_overlap_discovery_sources() {
        let config = AppleSpmConfigBuilder::new()
            .with_wrapper_sources(".")
            .build();
        let error = SpmPackageGenerator::new_local(&config, SpmLayout::Bundled)
            .render_package()
            .expect_err("overlapping wrapper and discovery sources must fail");
        assert!(
            error
                .to_string()
                .contains("overlaps generated FFI module discovery")
        );
    }

    #[test]
    fn binary_target_name_avoids_the_swift_wrapper_target_name() {
        let config = AppleSpmConfigBuilder::new()
            .with_module_name("MylibFFIBinary")
            .with_xcframework_name("Mylib")
            .with_ffi_module_name("MylibFFI")
            .build();
        let package = SpmPackageGenerator::new_local(&config, SpmLayout::Bundled)
            .render_package()
            .expect("render package");
        assert!(package.contains("name: \"MylibFFIBinaryBinary\""));
        assert!(package.contains("dependencies: [\"MylibFFIBinaryBinary\"]"));
    }

    #[test]
    fn spm_omits_macos_platform_when_enabled_without_macos_slices() {
        let config = AppleSpmConfigBuilder::new()
            .with_macos_enabled()
            .without_macos_slices()
            .build();

        let package = SpmPackageGenerator::new_local(&config, SpmLayout::FfiOnly)
            .render_package()
            .expect("render local package");

        assert!(package.contains(".iOS(.v16)"));
        assert!(!package.contains(".macOS(.v13)"));
    }

    #[test]
    fn spm_omits_ios_platform_for_macos_only_packaging() {
        let config = AppleSpmConfigBuilder::new()
            .with_macos_enabled()
            .without_ios_slices()
            .with_macos_arm64_slice()
            .build();

        let package = SpmPackageGenerator::new_local(&config, SpmLayout::FfiOnly)
            .render_package()
            .expect("render local package");

        assert!(!package.contains(".iOS(.v16)"));
        assert!(package.contains(".macOS(.v13)"));
    }

    #[test]
    fn spm_keeps_ios_platform_for_simulator_only_packaging() {
        let config = AppleSpmConfigBuilder::new()
            .without_ios_slices()
            .with_simulator_arm64_slice()
            .build();

        let package = SpmPackageGenerator::new_local(&config, SpmLayout::FfiOnly)
            .render_package()
            .expect("render local package");

        assert!(package.contains(".iOS(.v16)"));
        assert!(!package.contains(".macOS(.v13)"));
    }

    #[test]
    fn spm_renders_remote_package_from_template() {
        let config = AppleSpmConfigBuilder::new()
            .with_remote_spm("https://example.com/releases")
            .build();

        let package = SpmPackageGenerator::new_remote(
            &config,
            "abc123".to_string(),
            "1.2.3".to_string(),
            SpmLayout::Bundled,
        )
        .render_package()
        .expect("render remote package");

        assert!(package.contains("let releaseTag = \"1.2.3\""));
        assert!(package.contains("let releaseChecksum = \"abc123\""));
        assert!(package.contains(
            "url: \"https://example.com/releases/releases/download/\\(releaseTag)/Mylib.xcframework.zip\""
        ));
        assert!(package.contains("checksum: releaseChecksum"));
        assert!(package.contains(".target("));
        assert!(package.contains("path: \"Sources\""));
    }
}
