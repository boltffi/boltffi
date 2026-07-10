use std::env;
use std::path::{Path, PathBuf};

use boltffi_binding::{
    BINDING_EXPANSION_ROOT_ENV, BINDING_METADATA_FEATURES_ENV, BINDING_METADATA_ROOT_ENV,
};
use boltffi_ffi_rules::cargo_graph::ResolvedFeatures;
use boltffi_scan::{ActiveCfg, ScanConfiguration};

pub struct ScanEnvironment {
    configuration: ScanConfiguration,
}

impl ScanEnvironment {
    pub fn from_env() -> Self {
        let active = ActiveCfg::from_cargo_env();
        let configuration = match Self::resolved_features() {
            Some(features) => ScanConfiguration::from_resolved_features(active, features),
            None => ScanConfiguration::from_active_cfg(active),
        };
        Self { configuration }
    }

    pub fn configuration(&self) -> &ScanConfiguration {
        &self.configuration
    }

    pub fn into_configuration(self) -> ScanConfiguration {
        self.configuration
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ffi_rules::cargo_graph::ResolvedFeatures;
    use boltffi_scan::{ActiveCfg, ScanConfiguration};

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
}
