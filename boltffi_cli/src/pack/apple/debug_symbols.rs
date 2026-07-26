use std::path::PathBuf;

use crate::build::CargoBuildProfile;
use crate::cli::Result;
use crate::config::{Config, DebugSymbolsBundle, DebugSymbolsFormat};
use crate::pack::symbols::{
    DebugSymbolArtifact, DebugSymbolArtifactKind, ensure_debug_symbols_profile_has_debuginfo,
    ensure_existing_debug_symbol_artifacts_are_usable, write_debug_symbols_zip,
};
use crate::target::{BuiltLibrary, RustTarget};

pub struct DebugSymbols {
    enabled: bool,
    archive: Option<Archive>,
}

struct Archive {
    output: PathBuf,
    name: String,
    bundle: &'static str,
}

impl DebugSymbols {
    pub fn new(config: &Config) -> Self {
        Self {
            enabled: config.apple_debug_symbols_enabled(),
            archive: config
                .apple_debug_symbols_archive_enabled()
                .then(|| Archive::new(config)),
        }
    }
}

impl DebugSymbols {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn archive_enabled(&self) -> bool {
        self.archive.is_some()
    }

    pub fn validate_profile(
        &self,
        cargo_args: &[String],
        profile: &CargoBuildProfile,
        targets: &[RustTarget],
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        ensure_debug_symbols_profile_has_debuginfo(
            cargo_args,
            profile,
            "targets.apple.debug_symbols",
            &targets
                .iter()
                .map(|target| target.triple().to_string())
                .collect::<Vec<_>>(),
        )
    }

    pub fn validate_libraries(&self, libraries: &[BuiltLibrary]) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        ensure_existing_debug_symbol_artifacts_are_usable(
            &libraries
                .iter()
                .map(|library| library.path.clone())
                .collect::<Vec<_>>(),
            "targets.apple.debug_symbols",
        )
    }

    pub fn write_archive(&self, libraries: &[BuiltLibrary]) -> Result<()> {
        if let Some(archive) = &self.archive {
            archive.write(libraries)?;
        }

        Ok(())
    }
}

impl Archive {
    fn new(config: &Config) -> Self {
        let name = match config.apple_debug_symbols_format() {
            DebugSymbolsFormat::Zip => {
                format!("{}.xcframework.symbols.zip", config.xcframework_name())
            }
        };
        let bundle = match config.apple_debug_symbols_bundle() {
            DebugSymbolsBundle::Unstripped => "unstripped",
        };

        Self {
            output: config.apple_debug_symbols_output(),
            name,
            bundle,
        }
    }
}

impl Archive {
    fn write(&self, libraries: &[BuiltLibrary]) -> Result<()> {
        write_debug_symbols_zip(
            &self.output,
            &self.name,
            "apple",
            self.bundle,
            &libraries.iter().map(Self::artifact).collect::<Vec<_>>(),
        )?;

        Ok(())
    }

    fn artifact(library: &BuiltLibrary) -> DebugSymbolArtifact {
        let platform = library.target.platform();
        assert!(
            platform.is_apple(),
            "apple debug symbol archive received a non-apple library"
        );

        DebugSymbolArtifact {
            source_path: library.path.clone(),
            archive_path: PathBuf::from(platform.canonical_name())
                .join(library.target.triple())
                .join(
                    library
                        .path
                        .file_name()
                        .expect("built apple library should have a filename"),
                ),
            kind: DebugSymbolArtifactKind::Static,
            target_triple: Some(library.target.triple().to_string()),
            platform: Some(platform),
            architecture: Some(library.target.architecture()),
            abi: None,
            host_target: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::target::{BuiltLibrary, RustTarget};

    use super::Archive;

    #[test]
    fn archive_paths_preserve_apple_platform_and_target_identity() {
        [
            (RustTarget::IOS_ARM64, "ios"),
            (RustTarget::IOS_SIM_ARM64, "ios-simulator"),
            (RustTarget::MACOS_ARM64, "macos"),
        ]
        .into_iter()
        .for_each(|(target, platform)| {
            let artifact = Archive::artifact(&BuiltLibrary {
                target,
                path: PathBuf::from("target/libdemo.a"),
            });

            assert_eq!(
                artifact.archive_path,
                PathBuf::from(platform)
                    .join(target.triple())
                    .join("libdemo.a")
            );
        });
    }
}
