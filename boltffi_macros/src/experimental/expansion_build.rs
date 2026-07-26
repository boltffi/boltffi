use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use boltffi_ast::PackageInfo;
use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BINDING_METADATA_FEATURES_ENV, BindingMetadataSurface,
    LowerError, Native, Wasm32, lower_with_declarations,
};
use boltffi_scan::{ActiveCfg, ScanError, ScanInput};
use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;

use crate::experimental::{
    error::Error as ExpansionError, expander::Expander, expansion::Expansion,
    rust_api::RootModuleTypes,
};

pub enum Item {
    Inactive,
    Dependency,
    Preserve,
    Tokens(TokenStream),
    Error(TokenStream),
}

static EMITTED: AtomicBool = AtomicBool::new(false);

pub fn item() -> Item {
    if env::var_os(BINDING_EXPANSION_BUILD_ENV).is_none() {
        return Item::Inactive;
    }

    match Request::from_env() {
        Ok(Some(_)) if EMITTED.swap(true, Ordering::AcqRel) => Item::Preserve,
        Ok(Some(request)) => request
            .render()
            .map(Item::Tokens)
            .unwrap_or_else(|error| Item::Error(error.into_compile_error())),
        Ok(None) => Item::Dependency,
        Err(error) => Item::Error(error.into_compile_error()),
    }
}

struct Request {
    root: PathBuf,
    source: PathBuf,
    package: PackageInfo,
}

impl Request {
    fn from_env() -> Result<Option<Self>, BuildError> {
        if !compiling_requested_crate()? {
            return Ok(None);
        }

        Ok(Some(Self {
            root: requested_root()?,
            source: PathBuf::from(required_env(BINDING_EXPANSION_SOURCE_ENV)?),
            package: PackageInfo::new(
                required_env("CARGO_PKG_NAME")?,
                env::var("CARGO_PKG_VERSION")
                    .ok()
                    .filter(|version| !version.is_empty()),
            ),
        }))
    }

    fn render(self) -> Result<TokenStream, BuildError> {
        let scan = boltffi_scan::scan_package(
            &ScanInput::new(&self.source, self.package)
                .with_manifest_dir(&self.root)
                .with_cfg(active_cfg()),
        )?;
        let source = scan.root_with_support();
        let complete = scan.complete();
        let visible_paths = scan
            .root_visible_paths()
            .map(|(id, path)| (id.to_owned(), path.clone()))
            .collect::<Vec<_>>();
        let root_types =
            RootModuleTypes::with_visible_paths(&complete.package, visible_paths.clone());
        let source = root_types.contract(&source);
        let root = root_types.contract(scan.root());
        let expander = Expander::with_support(&root, &source, visible_paths);

        match requested_surface()? {
            BindingMetadataSurface::Native => {
                let lowered = lower_with_declarations::<Native>(&source)?;
                let expansion = Expansion::new(&lowered);
                expander.native(&expansion).map_err(Into::into)
            }
            BindingMetadataSurface::Wasm32 => {
                let lowered = lower_with_declarations::<Wasm32>(&source)?;
                let expansion = Expansion::new(&lowered);
                expander.wasm32(&expansion).map_err(Into::into)
            }
        }
    }
}

enum BuildError {
    MissingEnv(&'static str),
    InvalidSurface(String),
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
            Self::MissingEnv(key) => {
                write!(formatter, "BoltFFI IR expansion build: `{key}` is not set")
            }
            Self::InvalidSurface(surface) => {
                write!(
                    formatter,
                    "BoltFFI IR expansion build: `{BINDING_EXPANSION_SURFACE_ENV}` has invalid value `{surface}`"
                )
            }
            Self::Scan(error) => {
                write!(formatter, "BoltFFI IR expansion build scan failed: {error}")
            }
            Self::Lower(error) => {
                write!(
                    formatter,
                    "BoltFFI IR expansion build lower failed: {error}"
                )
            }
            Self::Expansion(error) => {
                write!(
                    formatter,
                    "BoltFFI IR expansion build wrapper emission failed: {error}"
                )
            }
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

fn requested_surface() -> Result<BindingMetadataSurface, BuildError> {
    let surface = required_env(BINDING_EXPANSION_SURFACE_ENV)?;
    BindingMetadataSurface::parse(&surface).ok_or(BuildError::InvalidSurface(surface))
}

fn compiling_requested_crate() -> Result<bool, BuildError> {
    let manifest_dir = PathBuf::from(required_env("CARGO_MANIFEST_DIR")?);
    Ok(canonical(&manifest_dir) == canonical(&requested_root()?))
}

fn requested_root() -> Result<PathBuf, BuildError> {
    Ok(PathBuf::from(required_env(BINDING_EXPANSION_ROOT_ENV)?))
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn active_cfg() -> ActiveCfg {
    let features = env::var(BINDING_METADATA_FEATURES_ENV)
        .ok()
        .into_iter()
        .flat_map(|features| {
            features
                .split(',')
                .filter(|feature| !feature.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    ActiveCfg::from_cargo_env().with_features(features)
}
