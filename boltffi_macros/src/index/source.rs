use std::path::Path;

use boltffi_scan::{ActiveCfg, SourceModule, SourceTree};
use proc_macro2::Span;

pub struct IndexedSource {
    root_path: Vec<String>,
    modules: Vec<SourceModule>,
}

impl IndexedSource {
    pub fn load(
        root_path: Vec<String>,
        source_file: &Path,
        module_name: &str,
        cfg: &ActiveCfg,
    ) -> syn::Result<Self> {
        SourceTree::load_with_cfg(source_file, module_name, cfg)
            .map(|source_tree| Self {
                root_path,
                modules: source_tree.modules().to_vec(),
            })
            .map_err(|error| syn::Error::new(Span::call_site(), error.to_string()))
    }

    pub fn root_path(&self) -> &[String] {
        &self.root_path
    }

    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }

    pub fn module_path<'module>(&self, module: &'module SourceModule) -> &'module [String] {
        module.path().get(1..).unwrap_or_default()
    }

    pub fn path(&self, module: &SourceModule) -> Vec<String> {
        self.root_path
            .iter()
            .cloned()
            .chain(self.module_path(module).iter().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_scan::ActiveCfg;

    use super::IndexedSource;

    #[test]
    fn loads_only_configured_modules_with_relative_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("boltffi-index-source-{unique}"));
        let root = directory.join("lib.rs");
        fs::create_dir_all(&directory).expect("create source directory");
        fs::write(
            &root,
            "#[cfg(any())] mod hidden; \
             #[cfg_attr(all(), path = \"bridge.rs\")] mod api;",
        )
        .expect("write root");
        fs::write(directory.join("bridge.rs"), "pub struct Visible;").expect("write bridge");
        fs::write(directory.join("hidden.rs"), "pub struct Hidden;").expect("write hidden");

        let source = IndexedSource::load(Vec::new(), &root, "demo", &ActiveCfg::default())
            .expect("load source");
        let paths = source
            .modules()
            .iter()
            .map(|module| source.module_path(module).join("::"))
            .collect::<Vec<_>>();
        fs::remove_dir_all(directory).expect("remove source directory");

        assert!(paths.contains(&"api".to_string()));
        assert!(!paths.contains(&"hidden".to_string()));
    }
}
