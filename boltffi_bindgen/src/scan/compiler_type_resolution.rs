use proc_macro2::Span;
use quote::quote;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use syn::{LitStr, Type};

fn runner_dir(crate_path: &Path) -> PathBuf {
    crate_path.join("target").join("boltffi_bindgen_type_resolution")
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), String> {
    let existing = fs::read_to_string(path).ok();
    if existing.as_deref() == Some(contents) {
        return Ok(());
    }
    fs::write(path, contents).map_err(|e| format!("write {}: {}", path.display(), e))
}

fn rewrite_crate_prefix(spelling: &str) -> Option<String> {
    spelling.strip_prefix("crate::").map(|rest| format!("target_crate::{}", rest))
}

fn generate_main_rs(spellings: &[(String, Type)]) -> String {
    let entries = spellings.iter().map(|(original, ty)| {
        let lit = LitStr::new(original, Span::call_site());
        quote! {
            {
                let canonical = ::std::any::type_name::<#ty>();
                let _ = writeln!(out, "{}\t{}", #lit, canonical);
            }
        }
    });

    quote! {
        use ::std::io::{self, Write};

        use target_crate as _;

        fn main() {
            let mut out = io::BufWriter::new(io::stdout());
            #(#entries)*
        }
    }
    .to_string()
}

#[derive(Clone, Debug, Deserialize)]
struct CargoMetadataJson {
    packages: Vec<CargoPackage>,
}

#[derive(Clone, Debug, Deserialize)]
struct CargoPackage {
    name: String,
    edition: String,
    manifest_path: String,
}

fn load_cargo_metadata(crate_path: &Path) -> Result<CargoMetadataJson, String> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .current_dir(crate_path)
        .output()
        .map_err(|e| format!("cargo metadata: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo metadata failed: {}", stderr.trim()));
    }

    serde_json::from_slice(&output.stdout).map_err(|e| format!("parse cargo metadata: {}", e))
}

fn select_target_package(crate_path: &Path, package_hint: &str, metadata: &CargoMetadataJson) -> Result<CargoPackage, String> {
    let canonical_manifest_path = crate_path.join("Cargo.toml").canonicalize().ok().and_then(|path| path.to_str().map(str::to_string));

    metadata
        .packages
        .iter()
        .find(|package| Some(package.manifest_path.as_str()) == canonical_manifest_path.as_deref())
        .cloned()
        .or_else(|| metadata.packages.iter().find(|package| package.name == package_hint).cloned())
        .ok_or_else(|| {
            let available = metadata
                .packages
                .iter()
                .map(|package| format!("{} ({})", package.name, package.manifest_path))
                .collect::<Vec<_>>()
                .join(", ");
            format!("could not select target package (hint: {}) from cargo metadata: {}", package_hint, available)
        })
}

fn cargo_manifest_dir(manifest_path: &str) -> Result<PathBuf, String> {
    PathBuf::from(manifest_path)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("invalid manifest path: {}", manifest_path))
}

pub fn resolve(crate_path: &Path, package_hint: &str, spellings: impl IntoIterator<Item = String>) -> Result<HashMap<String, String>, String> {
    let mut unique = HashSet::<String>::new();
    let mut targets = spellings
        .into_iter()
        .filter(|s| s.starts_with("crate::"))
        .filter(|s| unique.insert(s.clone()))
        .collect::<Vec<_>>();

    targets.sort();

    if targets.is_empty() {
        return Ok(HashMap::new());
    }

    let parsed = targets
        .iter()
        .filter_map(|original| {
            let rewritten = rewrite_crate_prefix(original)?;
            let ty = syn::parse_str::<Type>(&rewritten).ok()?;
            Some((original.clone(), ty))
        })
        .collect::<Vec<_>>();

    if parsed.is_empty() {
        return Ok(HashMap::new());
    }

    let metadata = load_cargo_metadata(crate_path)?;
    let target_package = select_target_package(crate_path, package_hint, &metadata)?;
    let target_manifest_dir = cargo_manifest_dir(&target_package.manifest_path)?;

    let dir = runner_dir(crate_path);
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).map_err(|e| format!("mkdir {}: {}", src_dir.display(), e))?;

    let cargo_toml = format!(
        "[package]\nname = \"boltffi_bindgen_type_resolution_runner\"\nversion = \"0.1.0\"\nedition = \"{}\"\n\n[dependencies]\ntarget_crate = {{ path = \"{}\", package = \"{}\" }}\n",
        target_package.edition,
        target_manifest_dir.display(),
        target_package.name,
    );

    write_if_changed(&dir.join("Cargo.toml"), &cargo_toml)?;
    write_if_changed(&src_dir.join("main.rs"), &generate_main_rs(&parsed))?;

    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .current_dir(&dir)
        .output()
        .map_err(|e| format!("run type resolution runner: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("type resolution runner failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map = stdout
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(spelling, canonical)| (spelling.to_string(), canonical.to_string()))
        .collect::<HashMap<_, _>>();

    Ok(map)
}
