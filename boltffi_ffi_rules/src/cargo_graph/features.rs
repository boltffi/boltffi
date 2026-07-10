use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CargoFeatureSelection {
    all: bool,
    default: bool,
    features: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ResolvedFeatures {
    names: BTreeSet<String>,
}

impl CargoFeatureSelection {
    pub fn exact(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            all: false,
            default: false,
            features: features.into_iter().map(Into::into).collect(),
        }
    }

    pub fn from_args(arguments: &[String]) -> Self {
        let mut skip_value = false;
        arguments
            .iter()
            .enumerate()
            .fold(Self::default(), |mut selection, (index, argument)| {
                if skip_value {
                    skip_value = false;
                    return selection;
                }

                match argument.as_str() {
                    "--all-features" => selection.all = true,
                    "--no-default-features" => selection.default = false,
                    "--features" | "-F" => {
                        skip_value = true;
                        arguments
                            .get(index + 1)
                            .into_iter()
                            .flat_map(|features| Self::split(features))
                            .for_each(|feature| {
                                selection.features.insert(feature);
                            });
                    }
                    _ => {
                        argument
                            .strip_prefix("--features=")
                            .map(Self::split)
                            .or_else(|| {
                                argument
                                    .strip_prefix("-F")
                                    .map(|features| Self::split(features.trim_start_matches('=')))
                            })
                            .into_iter()
                            .flatten()
                            .for_each(|feature| {
                                selection.features.insert(feature);
                            });
                    }
                }

                selection
            })
    }

    pub fn all(&self) -> bool {
        self.all
    }

    pub fn default_enabled(&self) -> bool {
        self.default
    }

    pub fn features(&self) -> impl Iterator<Item = &str> {
        self.features.iter().map(String::as_str)
    }

    pub fn cargo_arguments(&self) -> Vec<String> {
        if self.all {
            return vec!["--all-features".to_owned()];
        }

        (!self.default)
            .then(|| "--no-default-features".to_owned())
            .into_iter()
            .chain((!self.features.is_empty()).then(|| "--features".to_owned()))
            .chain(
                (!self.features.is_empty())
                    .then(|| self.features.iter().cloned().collect::<Vec<_>>().join(",")),
            )
            .collect()
    }

    pub fn resolve(&self, available: &BTreeMap<String, Vec<String>>) -> ResolvedFeatures {
        self.resolve_for_package("", available)
    }

    pub fn resolve_for_package(
        &self,
        package_name: &str,
        available: &BTreeMap<String, Vec<String>>,
    ) -> ResolvedFeatures {
        let mut names = if self.all {
            available.keys().cloned().collect::<BTreeSet<_>>()
        } else {
            self.features
                .iter()
                .filter_map(|feature| {
                    ResolvedFeatures::selected_local(feature, package_name, available)
                })
                .chain(
                    self.default
                        .then_some("default")
                        .filter(|feature| available.contains_key(*feature))
                        .map(str::to_owned),
                )
                .collect::<BTreeSet<_>>()
        };
        while let Some(feature) = names
            .iter()
            .filter_map(|feature| available.get(feature))
            .flat_map(|dependencies| dependencies.iter())
            .filter_map(|dependency| ResolvedFeatures::local_dependency(dependency, available))
            .find(|dependency| !names.contains(dependency))
        {
            names.insert(feature);
        }
        ResolvedFeatures { names }
    }

    fn split(features: &str) -> impl Iterator<Item = String> + '_ {
        features
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|feature| !feature.is_empty())
            .map(str::to_owned)
    }
}

impl ResolvedFeatures {
    pub fn exact(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            names: features.into_iter().map(Into::into).collect(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str)
    }

    pub fn selection(&self) -> CargoFeatureSelection {
        CargoFeatureSelection::exact(self.names.iter().cloned())
    }

    pub fn env_value(&self) -> String {
        self.iter().collect::<Vec<_>>().join(",")
    }

    fn local_dependency(
        dependency: &str,
        available: &BTreeMap<String, Vec<String>>,
    ) -> Option<String> {
        if dependency.starts_with("dep:") {
            return None;
        }
        let local = match dependency.split_once('/') {
            Some((dependency, _)) if dependency.ends_with('?') => return None,
            Some((dependency, _)) => dependency,
            None => dependency,
        };
        available.contains_key(local).then(|| local.to_owned())
    }

    fn selected_local(
        feature: &str,
        package_name: &str,
        available: &BTreeMap<String, Vec<String>>,
    ) -> Option<String> {
        if available.contains_key(feature) {
            return Some(feature.to_owned());
        }
        feature
            .split_once('/')
            .filter(|(package, _)| *package == package_name)
            .and_then(|(_, feature)| available.contains_key(feature).then(|| feature.to_owned()))
            .or_else(|| Self::local_dependency(feature, available))
    }
}

impl Default for CargoFeatureSelection {
    fn default() -> Self {
        Self {
            all: false,
            default: true,
            features: BTreeSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CargoFeatureSelection;

    #[test]
    fn selection_retains_only_cargo_feature_arguments() {
        let arguments = [
            "--release",
            "--features",
            "ffi,models",
            "--target",
            "aarch64-apple-darwin",
            "-Fdependency/ffi",
            "--no-default-features",
            "--all-features",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();

        let selection = CargoFeatureSelection::from_args(&arguments);

        assert!(selection.all());
        assert!(!selection.default_enabled());
        assert_eq!(
            selection.features().collect::<Vec<_>>(),
            vec!["dependency/ffi", "ffi", "models"]
        );
        assert_eq!(selection.cargo_arguments(), vec!["--all-features"]);
    }

    #[test]
    fn exact_selection_disables_implicit_defaults() {
        let selection = CargoFeatureSelection::exact(["default", "native-ffi"]);

        assert!(!selection.all());
        assert!(!selection.default_enabled());
        assert_eq!(
            selection.features().collect::<Vec<_>>(),
            vec!["default", "native-ffi"]
        );
        assert_eq!(
            selection.cargo_arguments(),
            vec!["--no-default-features", "--features", "default,native-ffi"]
        );
    }

    #[test]
    fn resolved_features_close_over_default_recursively() {
        let available = [
            ("default".to_owned(), vec!["api".to_owned()]),
            ("api".to_owned(), vec!["models".to_owned()]),
            ("models".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            CargoFeatureSelection::default()
                .resolve(&available)
                .iter()
                .collect::<Vec<_>>(),
            vec!["api", "default", "models"]
        );
    }

    #[test]
    fn explicit_dependency_syntax_does_not_activate_local_feature() {
        let available = [
            ("default".to_owned(), vec!["dep:transport".to_owned()]),
            ("transport".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            CargoFeatureSelection::default()
                .resolve(&available)
                .iter()
                .collect::<Vec<_>>(),
            vec!["default"]
        );
    }

    #[test]
    fn strong_dependency_feature_activates_local_dependency_feature() {
        let available = [
            ("default".to_owned(), vec!["transport/wire".to_owned()]),
            ("transport".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            CargoFeatureSelection::default()
                .resolve(&available)
                .iter()
                .collect::<Vec<_>>(),
            vec!["default", "transport"]
        );
    }

    #[test]
    fn weak_dependency_feature_does_not_activate_local_dependency_feature() {
        let available = [
            ("default".to_owned(), vec!["transport?/wire".to_owned()]),
            ("transport".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            CargoFeatureSelection::default()
                .resolve(&available)
                .iter()
                .collect::<Vec<_>>(),
            vec!["default"]
        );
    }

    #[test]
    fn selected_package_feature_resolves_to_root_feature() {
        let available = [
            ("ffi".to_owned(), Vec::new()),
            ("selected-package".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect();
        let selection = CargoFeatureSelection::exact(["selected-package/ffi"]);

        assert_eq!(
            selection
                .resolve_for_package("selected-package", &available)
                .iter()
                .collect::<Vec<_>>(),
            vec!["ffi"]
        );
    }

    #[test]
    fn dependency_feature_selection_keeps_dependency_semantics() {
        let available = [
            ("ffi".to_owned(), Vec::new()),
            ("transport".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect();
        let selection = CargoFeatureSelection::exact(["transport/wire"]);

        assert_eq!(
            selection
                .resolve_for_package("selected-package", &available)
                .iter()
                .collect::<Vec<_>>(),
            vec!["transport"]
        );
    }
}
