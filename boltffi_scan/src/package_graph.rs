use std::path::Path;

use boltffi_ffi_rules::cargo_graph::{
    CargoFeatureSelection, PackageGraph as CargoPackageGraph, ResolvedFeatures,
};
pub use boltffi_ffi_rules::cargo_graph::{LoadError, LocalPackage, PackageId};

pub struct ExportedPackageGraph {
    cargo: CargoPackageGraph,
}

impl ExportedPackageGraph {
    pub fn load(
        manifest_dir: &Path,
        root_module_name: &str,
        feature_selection: &CargoFeatureSelection,
    ) -> Result<Option<Self>, LoadError> {
        CargoPackageGraph::load_with_features(manifest_dir, root_module_name, feature_selection)
            .map(|graph| graph.map(|cargo| Self { cargo }))
    }

    pub fn root_id(&self) -> &PackageId {
        self.cargo.root_id()
    }

    pub fn root_features(&self) -> &ResolvedFeatures {
        self.cargo.root_features()
    }

    pub fn package(&self, id: &PackageId) -> Option<LocalPackage> {
        self.cargo.package(id)
    }

    pub fn reachable_exported_dependencies(&self, id: &PackageId) -> Vec<LocalPackage> {
        self.cargo.reachable_exported_dependencies(id)
    }

    pub fn direct_exported_dependencies(&self, id: &PackageId) -> Vec<LocalPackage> {
        self.cargo.exported_dependencies(id)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_ffi_rules::cargo_graph::CargoFeatureSelection;

    use super::ExportedPackageGraph;

    struct ExportGraphFixture {
        root: PathBuf,
        middle_source: PathBuf,
    }

    impl ExportGraphFixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after Unix epoch")
                .as_nanos();
            let directory =
                env::temp_dir().join(format!("boltffi-export-graph-{}-{stamp}", process::id()));
            let root = directory.join("root");
            let middle = directory.join("middle");
            let leaf = directory.join("leaf");
            [&root, &middle, &leaf]
                .into_iter()
                .map(|package| package.join("src"))
                .try_for_each(fs::create_dir_all)
                .expect("fixture package directories");
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"root\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmiddle = { path = \"../middle\" }\n",
            )
            .expect("root manifest");
            fs::write(
                middle.join("Cargo.toml"),
                "[package]\nname = \"middle\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nleaf = { path = \"../leaf\" }\n",
            )
            .expect("middle manifest");
            fs::write(
                leaf.join("Cargo.toml"),
                "[package]\nname = \"leaf\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
            )
            .expect("leaf manifest");
            fs::write(root.join("src/lib.rs"), "pub fn root() {}\n").expect("root source");
            let middle_source = middle.join("src/lib.rs");
            fs::write(&middle_source, "pub fn middle() {}\n").expect("middle source");
            fs::write(leaf.join("src/lib.rs"), "#[ffi_export]\npub fn leaf() {}\n")
                .expect("leaf source");
            Self {
                root,
                middle_source,
            }
        }

        fn graph(&self) -> ExportedPackageGraph {
            ExportedPackageGraph::load(&self.root, "root", &CargoFeatureSelection::default())
                .expect("package graph metadata")
                .expect("package graph")
        }

        fn mark_middle_exported(&self) {
            fs::write(&self.middle_source, "#[ffi_export]\npub fn middle() {}\n")
                .expect("middle source");
        }
    }

    impl Drop for ExportGraphFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(self.root.parent().expect("fixture directory")).ok();
        }
    }

    #[test]
    fn exported_reachability_does_not_cross_unexported_dependencies() {
        let fixture = ExportGraphFixture::new();
        let graph = fixture.graph();

        assert!(
            graph
                .reachable_exported_dependencies(graph.root_id())
                .is_empty()
        );

        fixture.mark_middle_exported();

        assert_eq!(
            graph
                .reachable_exported_dependencies(graph.root_id())
                .iter()
                .map(|package| package.module_name())
                .collect::<Vec<_>>(),
            ["leaf", "middle"]
        );
    }
}
