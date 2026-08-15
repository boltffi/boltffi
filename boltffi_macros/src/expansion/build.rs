use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use boltffi_ast::{PackageInfo, SourceContract, SourceFile};
use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BINDING_METADATA_BUILD_ENV, BINDING_METADATA_FEATURES_ENV,
    BINDING_METADATA_ROOT_ENV, BINDING_METADATA_SOURCE_ENV, BINDING_METADATA_SURFACE_ENV,
    BindingMetadataSurface, LowerError, Native, SerializedBindings, Wasm32,
    lower_with_declarations,
};
use boltffi_scan::{ActiveCfg, ScanError, ScanInput};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use serde::Deserialize;

use crate::data::scope::{DataId, Declaration};
use crate::expansion::{
    contract::Expansion, error::Error as ExpansionError, expander::Expander, metadata,
    rust_api::RootModuleTypes,
};

pub enum Item {
    Preserve,
    Tokens(TokenStream),
    Error(TokenStream),
}

pub enum DataItem {
    Tokens(DataExpansion),
    Error(TokenStream),
}

pub struct DataExpansion {
    runtime: TokenStream,
    root: Option<TokenStream>,
}

#[derive(Clone, Copy)]
enum Emission {
    Bindings,
    DataRuntime,
    Metadata,
    SourceOnly,
}

struct Request {
    manifest_dir: PathBuf,
    source: PathBuf,
    package: PackageInfo,
    surface: BindingMetadataSurface,
    emission: Emission,
}

#[derive(Debug, Eq, PartialEq)]
enum RustcCfg {
    Name(String),
    Value { name: String, value: String },
}

struct BuildContext {
    request: Request,
    root: SourceContract,
    support: SourceContract,
    visible_paths: Vec<(String, boltffi_ast::Path)>,
    data_source_files: HashMap<String, SourceFile>,
}

#[derive(Deserialize)]
struct CargoManifest {
    lib: Option<LibraryTarget>,
}

#[derive(Deserialize)]
struct LibraryTarget {
    path: Option<PathBuf>,
}

static EMITTED: AtomicBool = AtomicBool::new(false);
static CONTEXT: OnceLock<Result<BuildContext, String>> = OnceLock::new();

pub fn item() -> Item {
    if EMITTED.swap(true, Ordering::AcqRel) {
        return Item::Preserve;
    }
    context()
        .and_then(BuildContext::render)
        .map(Item::Tokens)
        .unwrap_or_else(|error| Item::Error(error.into_compile_error()))
}

pub fn data(declaration: &Declaration) -> DataItem {
    context()
        .and_then(|context| {
            let runtime = context.render_data(declaration)?;
            let root = (!EMITTED.swap(true, Ordering::AcqRel))
                .then(|| context.render())
                .transpose()?;
            Ok(DataExpansion { runtime, root })
        })
        .map(DataItem::Tokens)
        .unwrap_or_else(|error| DataItem::Error(error.into_compile_error()))
}

impl DataExpansion {
    pub fn runtime(&self) -> &TokenStream {
        &self.runtime
    }

    pub fn root(&self) -> Option<&TokenStream> {
        self.root.as_ref()
    }
}

fn context() -> Result<&'static BuildContext, BuildError> {
    CONTEXT
        .get_or_init(|| BuildContext::load().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| BuildError::Cached(message.clone()))
}

impl BuildContext {
    fn load() -> Result<Self, BuildError> {
        let request = Request::from_environment()?;
        let scan = boltffi_scan::scan_package(
            &ScanInput::new(&request.source, request.package.clone())
                .with_manifest_dir(&request.manifest_dir)
                .with_cfg(request.active_cfg()),
        )?;
        let visible_paths = scan
            .root_visible_paths()
            .map(|(id, path)| (id.to_owned(), path.clone()))
            .collect::<Vec<_>>();
        let data_source_files = scan
            .root()
            .records
            .iter()
            .map(|record| record.id.as_str())
            .chain(
                scan.root()
                    .enums
                    .iter()
                    .map(|enumeration| enumeration.id.as_str()),
            )
            .filter_map(|id| {
                scan.data_source_file(id)
                    .cloned()
                    .map(|source_file| (id.to_owned(), source_file))
            })
            .collect();
        let root_types =
            RootModuleTypes::with_visible_paths(&scan.complete().package, visible_paths.clone());
        let support = root_types.contract(&scan.root_with_support());
        let root = root_types.contract(scan.root());
        Ok(Self {
            request,
            root,
            support,
            visible_paths,
            data_source_files,
        })
    }

    fn render(&self) -> Result<TokenStream, BuildError> {
        let emitted_items = match self.request.emission {
            Emission::Bindings => self.render_root(),
            Emission::DataRuntime | Emission::SourceOnly => Ok(TokenStream::new()),
            Emission::Metadata => self.render_metadata(),
        }?;
        let expansion_build = BINDING_EXPANSION_BUILD_ENV;
        let expansion_root = BINDING_EXPANSION_ROOT_ENV;
        let expansion_source = BINDING_EXPANSION_SOURCE_ENV;
        let expansion_surface = BINDING_EXPANSION_SURFACE_ENV;
        let metadata_build = BINDING_METADATA_BUILD_ENV;
        let metadata_features = BINDING_METADATA_FEATURES_ENV;
        let metadata_root = BINDING_METADATA_ROOT_ENV;
        let metadata_source = BINDING_METADATA_SOURCE_ENV;
        let metadata_surface = BINDING_METADATA_SURFACE_ENV;
        Ok(quote! {
            const _: () = {
                let _ = ::core::option_env!(#expansion_build);
                let _ = ::core::option_env!(#expansion_root);
                let _ = ::core::option_env!(#expansion_source);
                let _ = ::core::option_env!(#expansion_surface);
                let _ = ::core::option_env!(#metadata_build);
                let _ = ::core::option_env!(#metadata_features);
                let _ = ::core::option_env!(#metadata_root);
                let _ = ::core::option_env!(#metadata_source);
                let _ = ::core::option_env!(#metadata_surface);
                let _ = ::core::option_env!("CARGO_PRIMARY_PACKAGE");
            };
            #emitted_items
        })
    }

    fn render_data(&self, declaration: &Declaration) -> Result<TokenStream, BuildError> {
        if matches!(
            self.request.emission,
            Emission::Metadata | Emission::SourceOnly
        ) {
            return Ok(TokenStream::new());
        }
        if let Some(scope) = declaration.local_scope() {
            let contract = boltffi_scan::scan_file(scope.clone(), self.request.package.clone())?;
            let id = declaration
                .resolve(&contract, |_| None)
                .ok_or_else(|| BuildError::MissingData(declaration.name().to_owned()))?;
            return self.render_data_id(&contract, id);
        }
        if let Some(id) = declaration.resolve(&self.support, |id| self.data_source_files.get(id)) {
            return self.render_data_id(&self.support, id);
        }
        let contract =
            boltffi_scan::scan_source(declaration.source(), self.request.package.clone())?;
        let root_types = RootModuleTypes::with_visible_paths(&contract.package, std::iter::empty());
        let contract = root_types.contract(&contract);
        let source_file = SourceFile::new(declaration.source().display().to_string());
        let id = declaration
            .resolve(&contract, |_| Some(&source_file))
            .ok_or_else(|| BuildError::MissingData(declaration.name().to_owned()))?;
        self.render_data_id(&contract, id)
    }

    fn render_root(&self) -> Result<TokenStream, BuildError> {
        let expander = Expander::with_support(
            &self.root,
            &self.support,
            self.visible_paths.iter().cloned(),
        );
        match self.request.surface {
            BindingMetadataSurface::Native => {
                let lowered = lower_with_declarations::<Native>(&self.support)?;
                expander
                    .native(&Expansion::new(&lowered))
                    .map_err(Into::into)
            }
            BindingMetadataSurface::Wasm32 => {
                let lowered = lower_with_declarations::<Wasm32>(&self.support)?;
                expander
                    .wasm32(&Expansion::new(&lowered))
                    .map_err(Into::into)
            }
        }
    }

    fn render_metadata(&self) -> Result<TokenStream, BuildError> {
        match self.request.surface {
            BindingMetadataSurface::Native => {
                let lowered = lower_with_declarations::<Native>(&self.support)?;
                metadata::render(SerializedBindings::native(lowered.into_bindings()))
                    .map_err(Into::into)
            }
            BindingMetadataSurface::Wasm32 => {
                let lowered = lower_with_declarations::<Wasm32>(&self.support)?;
                metadata::render(SerializedBindings::wasm32(lowered.into_bindings()))
                    .map_err(Into::into)
            }
        }
    }

    fn render_data_id(
        &self,
        source: &SourceContract,
        id: DataId,
    ) -> Result<TokenStream, BuildError> {
        let expander = Expander::with_support(source, source, std::iter::empty());
        match self.request.surface {
            BindingMetadataSurface::Native => {
                let lowered = lower_with_declarations::<Native>(source)?;
                let expansion = Expansion::new(&lowered);
                match id {
                    DataId::Record(id) => expander.record_runtime(&id, &expansion),
                    DataId::Enumeration(id) => expander.enumeration_runtime(&id, &expansion),
                }
                .map_err(Into::into)
            }
            BindingMetadataSurface::Wasm32 => {
                let lowered = lower_with_declarations::<Wasm32>(source)?;
                let expansion = Expansion::new(&lowered);
                match id {
                    DataId::Record(id) => expander.record_runtime(&id, &expansion),
                    DataId::Enumeration(id) => expander.enumeration_runtime(&id, &expansion),
                }
                .map_err(Into::into)
            }
        }
    }
}

impl Request {
    fn from_environment() -> Result<Self, BuildError> {
        if env::var_os(BINDING_EXPANSION_BUILD_ENV).is_some() {
            return Self::expansion_build();
        }
        if env::var_os(BINDING_METADATA_BUILD_ENV).is_some() {
            return Self::metadata_build();
        }
        Self::cargo_build()
    }

    fn expansion_build() -> Result<Self, BuildError> {
        let requested_root = PathBuf::from(required_env(BINDING_EXPANSION_ROOT_ENV)?);
        let manifest_dir = current_manifest_dir()?;
        let surface = parsed_surface(BINDING_EXPANSION_SURFACE_ENV)?;
        if canonical(&manifest_dir) == canonical(&requested_root) {
            return Self::new(
                manifest_dir,
                PathBuf::from(required_env(BINDING_EXPANSION_SOURCE_ENV)?),
                surface,
                Emission::Bindings,
            );
        }
        Self::local(manifest_dir, surface, Emission::DataRuntime)
    }

    fn metadata_build() -> Result<Self, BuildError> {
        let requested_root = PathBuf::from(required_env(BINDING_METADATA_ROOT_ENV)?);
        let manifest_dir = current_manifest_dir()?;
        let surface = parsed_surface(BINDING_METADATA_SURFACE_ENV)?;
        if canonical(&manifest_dir) == canonical(&requested_root) {
            return Self::new(
                manifest_dir,
                PathBuf::from(required_env(BINDING_METADATA_SOURCE_ENV)?),
                surface,
                Emission::Metadata,
            );
        }
        Self::local(manifest_dir, surface, Emission::SourceOnly)
    }

    fn cargo_build() -> Result<Self, BuildError> {
        let manifest_dir = current_manifest_dir()?;
        let surface = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("wasm32") => BindingMetadataSurface::Wasm32,
            _ => BindingMetadataSurface::Native,
        };
        let emission = match env::var_os("CARGO_PRIMARY_PACKAGE") {
            Some(_) => Emission::Bindings,
            None => Emission::DataRuntime,
        };
        Self::local(manifest_dir, surface, emission)
    }

    fn local(
        manifest_dir: PathBuf,
        surface: BindingMetadataSurface,
        emission: Emission,
    ) -> Result<Self, BuildError> {
        let manifest_path = manifest_dir.join("Cargo.toml");
        let manifest_source =
            fs::read_to_string(&manifest_path).map_err(|source| BuildError::ReadManifest {
                path: manifest_path.clone(),
                source,
            })?;
        let manifest = toml::from_str::<CargoManifest>(&manifest_source).map_err(|source| {
            BuildError::ParseManifest {
                path: manifest_path,
                source,
            }
        })?;
        let source = manifest.lib.and_then(|library| library.path).map_or_else(
            || manifest_dir.join("src/lib.rs"),
            |path| manifest_dir.join(path),
        );
        Self::new(manifest_dir, source, surface, emission)
    }

    fn new(
        manifest_dir: PathBuf,
        source: PathBuf,
        surface: BindingMetadataSurface,
        emission: Emission,
    ) -> Result<Self, BuildError> {
        Ok(Self {
            manifest_dir,
            source,
            package: PackageInfo::new(
                required_env("CARGO_PKG_NAME")?,
                env::var("CARGO_PKG_VERSION")
                    .ok()
                    .filter(|version| !version.is_empty()),
            ),
            surface,
            emission,
        })
    }

    fn active_cfg(&self) -> ActiveCfg {
        let features = matches!(self.emission, Emission::Bindings | Emission::Metadata)
            .then(|| env::var(BINDING_METADATA_FEATURES_ENV).ok())
            .flatten()
            .into_iter()
            .flat_map(|features| {
                features
                    .split(',')
                    .filter(|feature| !feature.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            });
        RustcCfg::from_arguments(env::args_os())
            .into_iter()
            .fold(ActiveCfg::from_cargo_env(), |active, compiler_cfg| {
                compiler_cfg.apply(active)
            })
            .with_features(features)
    }
}

impl RustcCfg {
    fn from_arguments(arguments: impl IntoIterator<Item = OsString>) -> Vec<Self> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        arguments
            .windows(2)
            .filter_map(|pair| {
                (pair[0].to_str() == Some("--cfg"))
                    .then(|| pair[1].to_str())
                    .flatten()
            })
            .chain(
                arguments
                    .iter()
                    .filter_map(|argument| argument.to_str()?.strip_prefix("--cfg=")),
            )
            .filter_map(Self::parse)
            .collect()
    }

    fn parse(argument: &str) -> Option<Self> {
        let Some((name, value)) = argument.split_once('=') else {
            return (!argument.is_empty()).then(|| Self::Name(argument.to_owned()));
        };
        let value = value.strip_prefix('"')?.strip_suffix('"')?;
        (!name.is_empty()).then(|| Self::Value {
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }

    fn apply(self, active: ActiveCfg) -> ActiveCfg {
        match self {
            Self::Name(name) => active.with_name(name),
            Self::Value { name, value } if name == "feature" => active.with_feature(value),
            Self::Value { name, value } => active.with_value(name, value),
        }
    }
}

enum BuildError {
    Cached(String),
    MissingEnv(&'static str),
    MissingData(String),
    InvalidSurface {
        key: &'static str,
        value: String,
    },
    ReadManifest {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseManifest {
        path: PathBuf,
        source: toml::de::Error,
    },
    Scan(ScanError),
    Lower(LowerError),
    Expansion(ExpansionError),
}

impl BuildError {
    fn into_compile_error(self) -> TokenStream {
        let message = self.to_string();
        quote_spanned! { Span::call_site() =>
            compile_error!(#message);
        }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cached(message) => formatter.write_str(message),
            Self::MissingEnv(key) => write!(formatter, "BoltFFI macro build: `{key}` is not set"),
            Self::MissingData(name) => write!(
                formatter,
                "BoltFFI macro build: data declaration `{name}` is missing from its Binding IR contract"
            ),
            Self::InvalidSurface { key, value } => {
                write!(
                    formatter,
                    "BoltFFI macro build: `{key}` has invalid value `{value}"
                )
            }
            Self::ReadManifest { path, source } => {
                write!(
                    formatter,
                    "read Cargo manifest `{}`: {source}",
                    path.display()
                )
            }
            Self::ParseManifest { path, source } => {
                write!(
                    formatter,
                    "parse Cargo manifest `{}`: {source}",
                    path.display()
                )
            }
            Self::Scan(error) => write!(formatter, "BoltFFI macro scan failed: {error}"),
            Self::Lower(error) => write!(formatter, "BoltFFI macro lowering failed: {error}"),
            Self::Expansion(error) => write!(formatter, "BoltFFI macro expansion failed: {error}"),
        }
    }
}

impl From<ScanError> for BuildError {
    fn from(error: ScanError) -> Self {
        Self::Scan(error)
    }
}

impl From<LowerError> for BuildError {
    fn from(error: LowerError) -> Self {
        Self::Lower(error)
    }
}

impl From<ExpansionError> for BuildError {
    fn from(error: ExpansionError) -> Self {
        Self::Expansion(error)
    }
}

fn required_env(key: &'static str) -> Result<String, BuildError> {
    env::var(key).map_err(|_| BuildError::MissingEnv(key))
}

fn current_manifest_dir() -> Result<PathBuf, BuildError> {
    required_env("CARGO_MANIFEST_DIR").map(PathBuf::from)
}

fn parsed_surface(key: &'static str) -> Result<BindingMetadataSurface, BuildError> {
    let value = required_env(key)?;
    BindingMetadataSurface::parse(&value).ok_or(BuildError::InvalidSurface { key, value })
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::RustcCfg;

    #[test]
    fn reads_separate_and_inline_compiler_cfg_arguments() {
        let cfg = RustcCfg::from_arguments(
            [
                "rustc",
                "--cfg",
                "test",
                "--cfg",
                "feature=\"experimental\"",
                "--cfg=target_feature=\"neon\"",
            ]
            .map(OsString::from),
        );

        assert_eq!(
            cfg,
            vec![
                RustcCfg::Name("test".to_owned()),
                RustcCfg::Value {
                    name: "feature".to_owned(),
                    value: "experimental".to_owned(),
                },
                RustcCfg::Value {
                    name: "target_feature".to_owned(),
                    value: "neon".to_owned(),
                },
            ]
        );
    }
}
