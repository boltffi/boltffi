use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use boltffi_binding::{
    BINDING_EXPANSION_ROOT_ENV, BINDING_METADATA_FEATURES_ENV, BINDING_METADATA_ROOT_ENV,
};
use boltffi_ffi_rules::cargo_graph::ResolvedFeatures;
use boltffi_scan::{ActiveCfg, RustcCfgArgs, RustcCfgArgsError, ScanConfiguration};

pub struct ScanEnvironment {
    configuration: ScanConfiguration,
}

#[derive(Debug)]
pub enum ScanEnvironmentError {
    RustcArguments(RustcCfgArgsError),
    FeatureMismatch {
        compiler: ResolvedFeatures,
        selected: ResolvedFeatures,
    },
}

impl ScanEnvironment {
    pub fn from_env() -> Result<Self, ScanEnvironmentError> {
        let rustc = RustcCfgArgs::from_args(env::args_os())?;
        Self::resolve(
            ActiveCfg::from_cargo_env(),
            rustc,
            Self::resolved_features(),
        )
    }

    pub fn configuration(&self) -> &ScanConfiguration {
        &self.configuration
    }

    pub fn into_configuration(self) -> ScanConfiguration {
        self.configuration
    }

    fn resolve(
        inherited: ActiveCfg,
        rustc: Option<RustcCfgArgs>,
        selected_features: Option<ResolvedFeatures>,
    ) -> Result<Self, ScanEnvironmentError> {
        let compiler_features = rustc
            .as_ref()
            .map(|rustc| ResolvedFeatures::exact(rustc.features()));
        if let (Some(compiler), Some(selected)) = (&compiler_features, &selected_features)
            && compiler != selected
        {
            return Err(ScanEnvironmentError::FeatureMismatch {
                compiler: compiler.clone(),
                selected: selected.clone(),
            });
        }
        let active = match rustc {
            Some(rustc) => rustc.resolve(inherited),
            None => inherited,
        };
        let configuration = match selected_features {
            Some(features) => ScanConfiguration::from_resolved_features(active, features),
            None => ScanConfiguration::from_active_cfg(active),
        };
        Ok(Self { configuration })
    }

    fn resolved_features() -> Option<ResolvedFeatures> {
        let current = env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from)?;
        let selected = [BINDING_METADATA_ROOT_ENV, BINDING_EXPANSION_ROOT_ENV]
            .into_iter()
            .filter_map(env::var_os)
            .map(PathBuf::from)
            .any(|root| Self::same_manifest(&current, &root));
        selected
            .then(|| env::var(BINDING_METADATA_FEATURES_ENV).ok())
            .flatten()
            .map(|features| {
                ResolvedFeatures::exact(features.split(',').filter(|feature| !feature.is_empty()))
            })
    }

    fn same_manifest(current: &Path, selected: &Path) -> bool {
        let selected = if selected
            .file_name()
            .is_some_and(|name| name == "Cargo.toml")
        {
            selected.parent().unwrap_or(selected)
        } else {
            selected
        };
        match (current.canonicalize(), selected.canonicalize()) {
            (Ok(current), Ok(selected)) => current == selected,
            _ => current == selected,
        }
    }
}

impl fmt::Display for ScanEnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RustcArguments(error) => {
                write!(
                    formatter,
                    "BoltFFI cannot read rustc configuration: {error}"
                )
            }
            Self::FeatureMismatch { compiler, selected } => {
                write!(
                    formatter,
                    "BoltFFI feature mismatch: rustc enables [{}], but binding generation selected [{}]",
                    compiler.env_value(),
                    selected.env_value()
                )
            }
        }
    }
}

impl std::error::Error for ScanEnvironmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RustcArguments(error) => Some(error),
            Self::FeatureMismatch { .. } => None,
        }
    }
}

impl From<RustcCfgArgsError> for ScanEnvironmentError {
    fn from(error: RustcCfgArgsError) -> Self {
        Self::RustcArguments(error)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ffi_rules::cargo_graph::ResolvedFeatures;
    use boltffi_scan::{ActiveCfg, RustcCfgArgs, ScanConfiguration};

    #[test]
    fn resolved_environment_preserves_exact_feature_identity() {
        let configuration = ScanConfiguration::from_resolved_features(
            ActiveCfg::default(),
            ResolvedFeatures::exact(["default", "native-ffi"]),
        );

        assert_eq!(
            configuration.active_cfg(),
            ActiveCfg::default().with_features(["default", "native-ffi"])
        );
        assert_ne!(
            configuration.active_cfg(),
            ActiveCfg::default().with_features(["default", "native_ffi"])
        );
    }

    #[test]
    fn feature_override_is_scoped_to_the_selected_manifest() {
        assert!(super::ScanEnvironment::same_manifest(
            Path::new("/workspace/root"),
            Path::new("/workspace/root/Cargo.toml")
        ));
        assert!(!super::ScanEnvironment::same_manifest(
            Path::new("/workspace/dependency"),
            Path::new("/workspace/root/Cargo.toml")
        ));
    }

    #[test]
    fn compiler_arguments_define_active_features_without_inventing_cargo_selection() {
        let rustc = RustcCfgArgs::from_args([
            "rustc",
            "--crate-name",
            "demo",
            "--cfg",
            "feature=\"boltffi\"",
            "--cfg",
            "feature=\"default\"",
        ])
        .expect("valid rustc arguments")
        .expect("rustc invocation");
        let environment = super::ScanEnvironment::resolve(
            ActiveCfg::default().with_feature("stale"),
            Some(rustc),
            None,
        )
        .expect("scan environment");

        assert_eq!(
            environment.configuration().active_cfg(),
            ActiveCfg::default().with_features(["boltffi", "default"])
        );
        let selection = environment.configuration().feature_selection();
        assert!(selection.default_enabled());
        assert!(selection.features().next().is_none());
    }

    #[test]
    fn conflicting_compiler_and_binding_features_are_rejected() {
        let rustc = RustcCfgArgs::from_args([
            "rustc",
            "--crate-name",
            "demo",
            "--cfg",
            "feature=\"active\"",
        ])
        .expect("valid rustc arguments")
        .expect("rustc invocation");
        let error = super::ScanEnvironment::resolve(
            ActiveCfg::default(),
            Some(rustc),
            Some(ResolvedFeatures::exact(["stale"])),
        )
        .err()
        .expect("feature mismatch");

        assert!(error.to_string().contains("rustc enables [active]"));
        assert!(error.to_string().contains("generation selected [stale]"));
    }

    #[test]
    fn matching_compiler_and_binding_features_share_one_configuration() {
        let rustc = RustcCfgArgs::from_args([
            "rustc",
            "--crate-name",
            "demo",
            "--cfg",
            "feature=\"active\"",
        ])
        .expect("valid rustc arguments")
        .expect("rustc invocation");
        let environment = super::ScanEnvironment::resolve(
            ActiveCfg::default(),
            Some(rustc),
            Some(ResolvedFeatures::exact(["active"])),
        )
        .expect("matching features");

        assert_eq!(
            environment.configuration().active_cfg(),
            ActiveCfg::default().with_feature("active")
        );
    }
}
