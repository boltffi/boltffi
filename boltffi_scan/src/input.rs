use std::path::{Path, PathBuf};

use boltffi_ast::PackageInfo;
use boltffi_ffi_rules::cargo_graph::{CargoFeatureSelection, ResolvedFeatures};

use crate::ActiveCfg;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScanConfiguration {
    active: ActiveCfg,
    features: FeatureActivation,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum FeatureActivation {
    Inherited,
    Selected(CargoFeatureSelection),
    Resolved(ResolvedFeatures),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScanInput {
    root: PathBuf,
    package: PackageInfo,
    manifest_dir: Option<PathBuf>,
    configuration: ScanConfiguration,
}

impl ScanConfiguration {
    pub fn from_active_cfg(active: ActiveCfg) -> Self {
        Self {
            active,
            features: FeatureActivation::Inherited,
        }
    }

    pub fn from_resolved_features(active: ActiveCfg, features: ResolvedFeatures) -> Self {
        let features = FeatureActivation::Resolved(features);
        Self {
            active: features.normalize(active),
            features,
        }
    }

    pub fn select_features(self, features: CargoFeatureSelection) -> Self {
        let features = FeatureActivation::Selected(features);
        Self {
            active: features.normalize(self.active),
            features,
        }
    }

    pub fn with_active_cfg(self, active: ActiveCfg) -> Self {
        Self {
            active: self.features.normalize(active),
            features: self.features,
        }
    }

    pub fn active_cfg(&self) -> ActiveCfg {
        self.features.configure(&self.active)
    }

    pub fn active_cfg_for_root(&self, features: &ResolvedFeatures) -> ActiveCfg {
        self.features.configure_root(&self.active, features)
    }

    pub fn feature_selection(&self) -> CargoFeatureSelection {
        self.features.selection()
    }

    pub fn resolved_features_for_root(
        &self,
        features: &ResolvedFeatures,
    ) -> Option<ResolvedFeatures> {
        self.features.resolved_for_root(features)
    }
}

impl FeatureActivation {
    fn normalize(&self, active: ActiveCfg) -> ActiveCfg {
        match self {
            Self::Inherited => active,
            Self::Selected(_) | Self::Resolved(_) => active.without_features(),
        }
    }

    fn configure(&self, active: &ActiveCfg) -> ActiveCfg {
        match self {
            Self::Inherited | Self::Selected(_) => active.clone(),
            Self::Resolved(features) => active.for_features(features.iter()),
        }
    }

    fn configure_root(&self, active: &ActiveCfg, root: &ResolvedFeatures) -> ActiveCfg {
        match self {
            Self::Inherited => active.clone(),
            Self::Selected(_) => active.for_features(root.iter()),
            Self::Resolved(features) => active.for_features(features.iter()),
        }
    }

    fn selection(&self) -> CargoFeatureSelection {
        match self {
            Self::Inherited => CargoFeatureSelection::default(),
            Self::Selected(features) => features.clone(),
            Self::Resolved(features) => features.selection(),
        }
    }

    fn resolved_for_root(&self, root: &ResolvedFeatures) -> Option<ResolvedFeatures> {
        match self {
            Self::Inherited => None,
            Self::Selected(_) => Some(root.clone()),
            Self::Resolved(features) => Some(features.clone()),
        }
    }
}

impl Default for ScanConfiguration {
    fn default() -> Self {
        Self::from_active_cfg(ActiveCfg::default())
            .select_features(CargoFeatureSelection::default())
    }
}

impl ScanInput {
    pub fn new(root: impl Into<PathBuf>, package: PackageInfo) -> Self {
        Self {
            root: root.into(),
            package,
            manifest_dir: None,
            configuration: ScanConfiguration::default(),
        }
    }

    pub fn with_manifest_dir(mut self, manifest_dir: impl Into<PathBuf>) -> Self {
        self.manifest_dir = Some(manifest_dir.into());
        self
    }

    pub fn with_cfg(mut self, cfg: ActiveCfg) -> Self {
        self.configuration = ScanConfiguration::from_active_cfg(cfg);
        self
    }

    pub fn with_configuration(mut self, configuration: ScanConfiguration) -> Self {
        self.configuration = configuration;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn package(&self) -> &PackageInfo {
        &self.package
    }

    pub fn manifest_dir(&self) -> Option<&Path> {
        self.manifest_dir.as_deref()
    }

    pub fn cfg(&self) -> &ActiveCfg {
        &self.configuration.active
    }

    pub fn configuration(&self) -> &ScanConfiguration {
        &self.configuration
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ffi_rules::cargo_graph::{CargoFeatureSelection, ResolvedFeatures};

    use super::ScanConfiguration;
    use crate::ActiveCfg;

    #[test]
    fn configuration_derives_exact_cfg_and_cargo_selection_together() {
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
        let selection = configuration.feature_selection();
        assert!(!selection.default_enabled());
        assert_eq!(
            selection.features().collect::<Vec<_>>(),
            vec!["default", "native-ffi"]
        );
    }

    #[test]
    fn selected_features_replace_inherited_feature_cfg() {
        let configuration = ScanConfiguration::from_active_cfg(
            ActiveCfg::default().with_feature("stale").with_name("unix"),
        )
        .select_features(CargoFeatureSelection::exact(["selected"]));
        let resolved = ResolvedFeatures::exact(["selected"]);

        assert_eq!(
            configuration.active_cfg_for_root(&resolved),
            ActiveCfg::default()
                .with_feature("selected")
                .with_name("unix")
        );
    }
}
