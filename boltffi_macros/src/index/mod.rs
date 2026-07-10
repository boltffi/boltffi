use boltffi_scan::{ActiveCfg, ExportedPackageGraph};
use proc_macro2::Span;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::scan_configuration::ScanEnvironment;

pub(crate) mod callback_traits;
pub(crate) mod class_types;
pub(crate) mod custom_types;
pub(crate) mod data_types;
mod path_resolver;
mod reexports;
mod source;
pub(crate) mod type_paths;

pub(crate) use path_resolver::PathResolver;

#[derive(Clone)]
pub(crate) struct CrateIndex {
    custom_types: custom_types::CustomTypeRegistry,
    class_types: class_types::ClassTypeRegistry,
    data_types: data_types::DataTypeRegistry,
    callback_traits: callback_traits::CallbackTraitRegistry,
    path_resolver: PathResolver,
}

static CRATE_INDEX_CACHE: OnceLock<Mutex<HashMap<CrateIndexKey, CrateIndex>>> = OnceLock::new();

#[derive(Clone, Eq, Hash, PartialEq)]
struct CrateIndexKey {
    manifest_dir: PathBuf,
    cfg: ActiveCfg,
}

impl CrateIndex {
    pub(crate) fn for_current_crate() -> syn::Result<Self> {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .map_err(|_| syn::Error::new(Span::call_site(), "CARGO_MANIFEST_DIR not set"))?;
        let environment = ScanEnvironment::from_env();
        let cfg = environment.configuration().active_cfg();
        let key = CrateIndexKey {
            manifest_dir: manifest_dir.clone(),
            cfg: cfg.clone(),
        };

        let cache = CRATE_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(crate_index) = cache
            .lock()
            .map_err(|_| syn::Error::new(Span::call_site(), "crate index lock poisoned"))?
            .get(&key)
            .cloned()
        {
            return Ok(crate_index);
        }

        let crate_name = env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "crate".to_owned());
        let feature_selection = environment.configuration().feature_selection();
        let graph = ExportedPackageGraph::load(&manifest_dir, &crate_name, &feature_selection)
            .map_err(|error| syn::Error::new(Span::call_site(), error.to_string()))?;
        let current_package = graph
            .as_ref()
            .and_then(|graph| graph.package(graph.root_id()));
        let source_file = current_package
            .as_ref()
            .map(|package| package.source_file().to_path_buf())
            .unwrap_or_else(|| manifest_dir.join("src/lib.rs"));
        let module_name = current_package
            .as_ref()
            .map(|package| package.module_name().to_owned())
            .unwrap_or(crate_name);
        let current = source::IndexedSource::load(Vec::new(), &source_file, &module_name, &cfg)?;
        let path_resolver = PathResolver::build(current.modules());
        let indexed_sources = Self::indexed_sources(current, graph.as_ref(), &cfg)?;
        let crate_index = Self {
            custom_types: custom_types::build_custom_type_registry(&indexed_sources)?,
            class_types: class_types::build_class_type_registry(
                &indexed_sources,
                path_resolver.clone(),
            )?,
            data_types: data_types::build_data_type_registry(
                &indexed_sources,
                path_resolver.clone(),
            )?,
            callback_traits: callback_traits::build_callback_trait_registry(&indexed_sources)?,
            path_resolver,
        };

        cache
            .lock()
            .map_err(|_| syn::Error::new(Span::call_site(), "crate index lock poisoned"))?
            .insert(key, crate_index.clone());

        Ok(crate_index)
    }

    fn indexed_sources(
        current: source::IndexedSource,
        graph: Option<&ExportedPackageGraph>,
        cfg: &ActiveCfg,
    ) -> syn::Result<Vec<source::IndexedSource>> {
        let dependency_cfg = cfg.without_features();
        let dependency_sources = graph
            .map(|graph| graph.reachable_exported_dependencies(graph.root_id()))
            .unwrap_or_default()
            .into_iter()
            .map(|package| {
                source::IndexedSource::load(
                    package.root_path(),
                    package.source_file(),
                    package.module_name(),
                    &dependency_cfg,
                )
            })
            .collect::<syn::Result<Vec<_>>>()?;

        Ok(std::iter::once(current).chain(dependency_sources).collect())
    }

    pub(crate) fn custom_types(&self) -> &custom_types::CustomTypeRegistry {
        &self.custom_types
    }

    pub(crate) fn data_types(&self) -> &data_types::DataTypeRegistry {
        &self.data_types
    }

    pub(crate) fn callback_traits(&self) -> &callback_traits::CallbackTraitRegistry {
        &self.callback_traits
    }

    pub(crate) fn class_types(&self) -> &class_types::ClassTypeRegistry {
        &self.class_types
    }

    pub(crate) fn path_resolver(&self) -> &PathResolver {
        &self.path_resolver
    }
}
