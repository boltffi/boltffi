use std::fs;
use std::path::{Path, PathBuf};

use boltffi_backend::bridge::c::CBridge;
use boltffi_backend::core::{CoverageMode, FilePath, GeneratedFile, bridge, host};
use boltffi_backend::target::python::PythonCExtHost;
use boltffi_backend::target::ruby::RubyCExtHost;
use boltffi_backend::{GeneratedOutput, Target as BackendTarget};
use boltffi_binding::{BindingMetadataSurface, Bindings, Native, Surface};
use thiserror::Error;

use crate::metadata::{BindingMetadataBuild, BindingMetadataBuildError};
use crate::target::Target;

/// Drives one BoltFFI generation from a compiled crate's embedded metadata
/// to rendered target-language files.
///
/// The driver runs the metadata build, selects the binding contract for the
/// target surface, renders it through the supplied [`Target`], and returns
/// the generated output. It carries no language-specific knowledge: the host
/// and bridge stack inside the [`Target`] decide everything about the
/// produced files.
#[derive(Clone, Debug)]
pub struct Generation {
    manifest_path: PathBuf,
    triple: Option<String>,
    coverage: CoverageMode,
    cargo_args: Vec<String>,
    python_package_module: Option<String>,
    python_distribution_name: Option<String>,
    python_package_version: Option<String>,
    python_native_library: Option<String>,
    ruby_ractor_safe: bool,
    ruby_extra_files: Vec<PathBuf>,
}

impl Generation {
    /// Creates a generation for a Cargo manifest.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            triple: None,
            coverage: CoverageMode::Complete,
            cargo_args: Vec::new(),
            python_package_module: None,
            python_distribution_name: None,
            python_package_version: None,
            python_native_library: None,
            ruby_ractor_safe: false,
            ruby_extra_files: Vec::new(),
        }
    }

    /// Builds for a Cargo target triple.
    pub fn triple(mut self, triple: impl Into<String>) -> Self {
        self.triple = Some(triple.into());
        self
    }

    /// Passes Cargo build arguments to metadata generation.
    pub fn cargo_args(mut self, cargo_args: impl IntoIterator<Item = String>) -> Self {
        self.cargo_args = cargo_args.into_iter().collect();
        self
    }

    /// Sets how unsupported backend declarations are handled.
    pub fn coverage_mode(mut self, coverage: CoverageMode) -> Self {
        self.coverage = coverage;
        self
    }

    /// Sets the generated Python package module name.
    pub fn python_module_name(mut self, module_name: impl Into<String>) -> Self {
        self.python_package_module = Some(module_name.into());
        self
    }

    /// Sets the generated Python distribution name.
    pub fn python_distribution_name(mut self, distribution_name: impl Into<String>) -> Self {
        self.python_distribution_name = Some(distribution_name.into());
        self
    }

    /// Sets the generated Python package version.
    pub fn python_package_version(mut self, package_version: Option<String>) -> Self {
        self.python_package_version = package_version;
        self
    }

    /// Sets the native library artifact name loaded by the Python package.
    pub fn python_native_library(mut self, native_library: impl Into<String>) -> Self {
        self.python_native_library = Some(native_library.into());
        self
    }

    /// Declares the generated Ruby extension Ractor-safe (`rb_ext_ractor_safe(true)`).
    pub fn ruby_ractor_safe(mut self, ractor_safe: bool) -> Self {
        self.ruby_ractor_safe = ractor_safe;
        self
    }

    /// Sets extra Ruby source files to include in the generated package.
    ///
    /// Each path is relative to the crate root. Files are copied into
    /// `lib/<crate_stem>/` and wired into the generated `lib/<crate>.rb`
    /// via `require_relative` and into the gemspec `spec.files`.
    pub fn ruby_extra_files(mut self, files: Vec<PathBuf>) -> Self {
        self.ruby_extra_files = files;
        self
    }

    /// Reads the embedded metadata, selects the target surface contract, and renders it.
    pub fn render(&self, target: Target) -> Result<GeneratedOutput, GenerationError> {
        match target {
            Target::Python => self.render_python(),
            Target::Ruby => self.render_ruby(),
            Target::Swift
            | Target::Kotlin
            | Target::KotlinMultiplatform
            | Target::Java
            | Target::TypeScript
            | Target::Header
            | Target::Dart
            | Target::CSharp => Err(GenerationError::UnsupportedTarget { target }),
        }
    }

    /// Renders the bindings and writes every generated file under `output_dir`.
    pub fn write(
        &self,
        target: Target,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        let output = self.render(target)?;
        Self::write_output(output, output_dir)
    }

    fn render_python(&self) -> Result<GeneratedOutput, GenerationError> {
        let bindings = self.bindings::<Native>()?;
        let target = self
            .python_host()?
            .into_target(&bindings)
            .map_err(GenerationError::Render)?;
        self.render_backend(&target, &bindings)
    }

    fn render_ruby(&self) -> Result<GeneratedOutput, GenerationError> {
        // Validate extra files before rendering so we never write generated
        // files that reference extras which haven't been validated yet.
        let extra_entries = self.validate_ruby_extra_files()?;

        let bindings = self.bindings::<Native>()?;
        let crate_stem = bindings
            .package()
            .name()
            .as_path_string()
            .replace("::", "_");
        let target = BackendTarget::new(
            RubyCExtHost::new().ractor_safe(self.ruby_ractor_safe),
            CBridge::new(format!("ext/{crate_stem}/boltffi.h")).map_err(GenerationError::Render)?,
        );
        let mut output = self.render_backend(&target, &bindings)?;

        if !extra_entries.is_empty() {
            apply_ruby_extra_files(&mut output, &crate_stem, &extra_entries)?;
        }

        Ok(output)
    }

    fn render_backend<H, S>(
        &self,
        target: &BackendTarget<H, S>,
        bindings: &Bindings<S::Surface>,
    ) -> Result<GeneratedOutput, GenerationError>
    where
        H: host::HostBackend<Bridge = S::Contract, Surface = S::Surface>,
        S: bridge::BridgeStack,
    {
        target
            .render_with_coverage(bindings, self.coverage)
            .map_err(GenerationError::Render)
    }

    fn python_host(&self) -> Result<PythonCExtHost, GenerationError> {
        let host = self
            .python_package_module
            .as_deref()
            .map(|module| PythonCExtHost::new().module_name(module))
            .transpose()
            .map_err(GenerationError::Render)
            .map(Option::unwrap_or_default)?;
        let host = self
            .python_distribution_name
            .iter()
            .fold(host, |host, name| host.distribution_name(name.clone()));
        let host = self
            .python_native_library
            .iter()
            .fold(host, |host, library| host.native_library(library.clone()));
        Ok(host.version(self.python_package_version.clone()))
    }

    /// Writes generated output to a directory.
    pub fn write_output(
        output: GeneratedOutput,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        output
            .files()
            .iter()
            .map(|file| {
                let path = output_dir.join(file.path().as_path());
                write_file(&path, file.contents())?;
                Ok(path)
            })
            .collect()
    }

    fn validate_ruby_extra_files(&self) -> Result<Vec<ExtraFileEntry>, GenerationError> {
        if self.ruby_extra_files.is_empty() {
            return Ok(Vec::new());
        }

        let crate_root = self
            .manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."));

        let mut entries = Vec::new();
        let mut seen_destinations = std::collections::HashSet::new();

        for source_rel in &self.ruby_extra_files {
            let source = crate_root.join(source_rel);

            // Check existence.
            if !source.exists() {
                return Err(GenerationError::ExtraFile {
                    path: source_rel.clone(),
                    message: "source file does not exist".to_string(),
                });
            }

            // Reject directories.
            if !source.is_file() {
                return Err(GenerationError::ExtraFile {
                    path: source_rel.clone(),
                    message: "source path is a directory, not a file".to_string(),
                });
            }

            // Check .rb extension.
            if source.extension().and_then(|e| e.to_str()) != Some("rb") {
                return Err(GenerationError::ExtraFile {
                    path: source_rel.clone(),
                    message: "extra Ruby files must have a .rb extension".to_string(),
                });
            }

            // Reject path traversal: normalized source must stay under crate root.
            let normalized_source =
                std::fs::canonicalize(&source).unwrap_or_else(|_| source.clone());
            let normalized_root =
                std::fs::canonicalize(crate_root).unwrap_or_else(|_| crate_root.to_path_buf());
            if normalized_source.strip_prefix(&normalized_root).is_err() {
                return Err(GenerationError::ExtraFile {
                    path: source_rel.clone(),
                    message: "source path escapes crate root".to_string(),
                });
            }

            // Compute destination: lib/<crate_stem>/<relative_path>.
            // When the configured path contains a `ruby/` component, preserve
            // the path below that component so `src/ruby/compat/foo.rb` becomes
            // `lib/<crate_stem>/compat/foo.rb`. Otherwise, preserve the
            // configured relative path under `lib/<crate_stem>/`.
            let destination_relative = ruby_extra_destination_relative(source_rel);
            let dest_key = destination_relative.to_string_lossy().to_string();

            if !seen_destinations.insert(dest_key.clone()) {
                return Err(GenerationError::ExtraFile {
                    path: source_rel.clone(),
                    message: format!("duplicate destination path: `{dest_key}`"),
                });
            }

            // Read contents now so we can add them as GeneratedFiles.
            let contents =
                std::fs::read_to_string(&source).map_err(|e| GenerationError::ExtraFile {
                    path: source_rel.clone(),
                    message: format!("failed to read: {e}"),
                })?;

            entries.push(ExtraFileEntry {
                destination_relative,
                contents,
            });
        }

        Ok(entries)
    }

    fn bindings<S: Surface>(&self) -> Result<Bindings<S>, GenerationError> {
        let surface = BindingMetadataSurface::from_target_triple(self.triple.as_deref());
        let mut build = BindingMetadataBuild::new(&self.manifest_path);
        if !self.cargo_args.is_empty() {
            build = build.cargo_args(self.cargo_args.clone());
        }
        if let Some(triple) = &self.triple {
            build = build.target(triple);
        }
        build
            .read()?
            .into_iter()
            .find(|envelope| envelope.surface() == surface)
            .and_then(|envelope| S::from_serialized(envelope.into_bindings()))
            .ok_or(GenerationError::MissingSurface { surface })
    }
}

/// Failure while generating bindings from embedded crate metadata.
#[derive(Debug, Error)]
pub enum GenerationError {
    /// The metadata build or artifact read failed.
    #[error(transparent)]
    Metadata(#[from] BindingMetadataBuildError),
    /// The compiled crate embedded no metadata for the requested surface.
    #[error("compiled crate embeds no binding metadata for the {surface:?} surface")]
    MissingSurface {
        /// Surface selected from the target triple.
        surface: BindingMetadataSurface,
    },
    /// The target backend failed to render the bindings.
    #[error("render bindings: {0}")]
    Render(boltffi_backend::Error),
    /// The target is not wired to the IR generation pipeline.
    #[error("IR generation is not available for {target}")]
    UnsupportedTarget {
        /// Requested target.
        target: Target,
    },
    /// A generated file could not be written to disk.
    #[error("write generated file `{path}`: {source}")]
    Write {
        /// Generated file path.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
    /// An extra Ruby file configured in `boltffi.toml` failed validation.
    #[error("extra Ruby file `{path}`: {message}")]
    ExtraFile {
        /// The configured source path.
        path: PathBuf,
        /// Human-readable validation error.
        message: String,
    },
}

fn ruby_extra_destination_relative(source_rel: &Path) -> PathBuf {
    let components: Vec<_> = source_rel.components().collect();
    let after_ruby = components
        .iter()
        .rposition(|component| component.as_os_str() == "ruby")
        .and_then(|index| {
            let tail = components[index + 1..]
                .iter()
                .map(|component| component.as_os_str())
                .collect::<PathBuf>();
            (!tail.as_os_str().is_empty()).then_some(tail)
        });

    after_ruby.unwrap_or_else(|| source_rel.to_path_buf())
}

/// A validated extra Ruby file entry ready for inclusion in generated output.
#[derive(Debug)]
struct ExtraFileEntry {
    /// Destination path under `lib/<crate_stem>/`.
    destination_relative: PathBuf,
    /// File contents read at validation time.
    contents: String,
}

/// Patches a generated Ruby output to include extra files.
///
/// This post-processes the rendered output rather than threading through the
/// template layer, because `Target<H, S>` derives `Copy` which prevents
/// storing `Vec<PathBuf>` on `RubyCExtHost`.
fn apply_ruby_extra_files(
    output: &mut GeneratedOutput,
    crate_stem: &str,
    entries: &[ExtraFileEntry],
) -> Result<(), GenerationError> {
    let (mut files, diagnostics, coverage) = output.clone().into_parts();

    // Compute require_relative paths and gemspec entries.
    let mut extra_requires: Vec<String> = Vec::new();
    let mut extra_gemspec_files: Vec<String> = Vec::new();

    for entry in entries {
        // Destination: lib/<crate_stem>/<destination_relative>
        let dest_path =
            PathBuf::from(format!("lib/{crate_stem}")).join(&entry.destination_relative);

        // require_relative path (relative to lib/<crate>.rb, without .rb extension)
        let mut require_relative = entry.destination_relative.clone();
        require_relative.set_extension("");
        let require_path = format!("{crate_stem}/{}", require_relative.to_string_lossy());
        extra_requires.push(require_path);

        // gemspec file entry (relative to gem root)
        let gemspec_entry = dest_path.to_string_lossy().to_string();
        extra_gemspec_files.push(gemspec_entry);

        // Add the extra file as a GeneratedFile.
        let file_path = FilePath::new(&dest_path).map_err(GenerationError::Render)?;
        files.push(GeneratedFile::new(file_path, entry.contents.clone()));
    }

    // Patch lib/<crate>.rb: add require_relative lines after the final `end`.
    let lib_path = format!("lib/{crate_stem}.rb");
    for file in &mut files {
        if file.path().as_path() == Path::new(&lib_path) {
            let new_contents = patch_lib_rb(file.contents(), &extra_requires);
            let path = FilePath::new(file.path().as_path()).map_err(GenerationError::Render)?;
            *file = GeneratedFile::new(path, new_contents);
            break;
        }
    }

    // Patch <crate_stem>.gemspec: add extra spec.files and require_paths.
    let gemspec_path = format!("{crate_stem}.gemspec");
    for file in &mut files {
        if file.path().as_path() == Path::new(&gemspec_path) {
            let new_contents = patch_gemspec(file.contents(), &extra_gemspec_files);
            let path = FilePath::new(file.path().as_path()).map_err(GenerationError::Render)?;
            *file = GeneratedFile::new(path, new_contents);
            break;
        }
    }

    // Reconstruct the output.
    *output = GeneratedOutput::new(files, diagnostics.to_vec()).with_coverage(coverage);
    Ok(())
}

/// Appends `require_relative` lines after the final `end` in `lib/<crate>.rb`.
/// Returns the modified contents.
fn patch_lib_rb(contents: &str, extra_requires: &[String]) -> String {
    let lines: Vec<&str> = contents.lines().collect();

    // Find the last `end` line (the module closing).
    let last_end = lines.iter().rposition(|line| line.trim() == "end");

    let require_lines: Vec<String> = extra_requires
        .iter()
        .map(|path| format!("require_relative \"{path}\""))
        .collect();

    if let Some(idx) = last_end {
        // Insert after the final `end`.
        let mut new_lines: Vec<String> = Vec::new();
        new_lines.extend(lines[..=idx].iter().map(|s| s.to_string()));
        new_lines.extend(require_lines);
        new_lines.extend(lines[idx + 1..].iter().map(|s| s.to_string()));
        let mut result = new_lines.join("\n");
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result
    } else {
        // No `end` found — just append.
        let mut result = contents.to_string();
        result.push('\n');
        for path in extra_requires {
            result.push_str(&format!("require_relative \"{path}\"\n"));
        }
        result
    }
}

/// Patches the gemspec to include extra files in `spec.files` and emit
/// `spec.require_paths = ["lib"]`.
///
/// Returns the modified contents.
fn patch_gemspec(contents: &str, extra_gemspec_files: &[String]) -> String {
    let extra_entries: String = extra_gemspec_files
        .iter()
        .map(|f| format!("    \"{f}\",\n"))
        .collect();

    // Insert extra file entries into spec.files array, right after the opening `[`.
    let mut new_contents = if contents.contains("spec.files = [\n") {
        contents.replacen(
            "spec.files = [\n",
            &format!("spec.files = [\n{extra_entries}"),
            1,
        )
    } else {
        contents.to_string()
    };

    // Add spec.require_paths = ["lib"] if not present.
    if !new_contents.contains("spec.require_paths") {
        // Insert before spec.extensions.
        if new_contents.contains("spec.extensions") {
            new_contents = new_contents.replacen(
                "  spec.extensions",
                "  spec.require_paths = [\"lib\"]\n\n  spec.extensions",
                1,
            );
        } else {
            // Fallback: append at end.
            new_contents.push_str("\n  spec.require_paths = [\"lib\"]\n");
        }
    }

    if !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }
    new_contents
}

fn write_file(path: &Path, contents: &str) -> Result<(), GenerationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| GenerationError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, contents).map_err(|source| GenerationError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_crate() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("boltffi-extra-files-{unique}-{counter}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn ruby_extra_destination_strips_ruby_component() {
        assert_eq!(
            ruby_extra_destination_relative(Path::new("src/ruby/compat/foo.rb")),
            PathBuf::from("compat/foo.rb")
        );
    }

    #[test]
    fn ruby_extra_destination_preserves_non_ruby_path() {
        assert_eq!(
            ruby_extra_destination_relative(Path::new("shim/compat.rb")),
            PathBuf::from("shim/compat.rb")
        );
    }

    #[test]
    fn ruby_extra_files_patch_lib_and_gemspec() {
        let mut output = GeneratedOutput::new(
            vec![
                GeneratedFile::new(
                    FilePath::new("lib/demo.rb").unwrap(),
                    "# frozen_string_literal: true\n\nrequire \"demo_native\"\n\nmodule Demo\nend\n",
                ),
                GeneratedFile::new(
                    FilePath::new("demo.gemspec").unwrap(),
                    "Gem::Specification.new do |spec|\n  spec.files = [\n    \"lib/demo.rb\",\n  ]\n\n  spec.extensions = [\"ext/demo/extconf.rb\"]\nend\n",
                ),
            ],
            Vec::new(),
        );
        let entries = vec![ExtraFileEntry {
            destination_relative: PathBuf::from("compat/foo.rb"),
            contents: "# compat\n".to_string(),
        }];

        apply_ruby_extra_files(&mut output, "demo", &entries).unwrap();
        let lib = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("lib/demo.rb"))
            .unwrap()
            .contents();
        assert!(lib.contains("end\nrequire_relative \"demo/compat/foo\"\n"));
        let gemspec = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new("demo.gemspec"))
            .unwrap()
            .contents();
        assert!(gemspec.contains("\"lib/demo/compat/foo.rb\","));
        assert!(gemspec.contains("spec.require_paths = [\"lib\"]"));
        assert!(
            output
                .files()
                .iter()
                .any(|file| file.path().as_path() == Path::new("lib/demo/compat/foo.rb"))
        );
    }

    #[test]
    fn ruby_extra_files_empty_list_is_ok() {
        let dir = temp_crate();
        let generation = Generation::new(dir.join("Cargo.toml"));
        let entries = generation.validate_ruby_extra_files().unwrap();
        assert!(entries.is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ruby_extra_files_rejects_missing_source() {
        let dir = temp_crate();
        let generation = Generation::new(dir.join("Cargo.toml"))
            .ruby_extra_files(vec![PathBuf::from("missing.rb")]);
        let error = generation
            .validate_ruby_extra_files()
            .unwrap_err()
            .to_string();
        assert!(error.contains("source file does not exist"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ruby_extra_files_rejects_directory() {
        let dir = temp_crate();
        std::fs::create_dir_all(dir.join("src/ruby/compat.rb")).unwrap();
        let generation = Generation::new(dir.join("Cargo.toml"))
            .ruby_extra_files(vec![PathBuf::from("src/ruby/compat.rb")]);
        let error = generation
            .validate_ruby_extra_files()
            .unwrap_err()
            .to_string();
        assert!(error.contains("source path is a directory"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ruby_extra_files_rejects_non_rb_extension() {
        let dir = temp_crate();
        std::fs::write(dir.join("compat.txt"), "nope").unwrap();
        let generation = Generation::new(dir.join("Cargo.toml"))
            .ruby_extra_files(vec![PathBuf::from("compat.txt")]);
        let error = generation
            .validate_ruby_extra_files()
            .unwrap_err()
            .to_string();
        assert!(error.contains(".rb extension"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ruby_extra_files_rejects_path_traversal() {
        let dir = temp_crate();
        let outside = dir.parent().unwrap().join("outside.rb");
        std::fs::write(&outside, "# outside\n").unwrap();
        let generation = Generation::new(dir.join("Cargo.toml"))
            .ruby_extra_files(vec![PathBuf::from("../outside.rb")]);
        let error = generation
            .validate_ruby_extra_files()
            .unwrap_err()
            .to_string();
        assert!(error.contains("escapes crate root"));
        std::fs::remove_file(outside).ok();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ruby_extra_files_rejects_duplicate_destinations() {
        let dir = temp_crate();
        std::fs::create_dir_all(dir.join("a/ruby/compat")).unwrap();
        std::fs::create_dir_all(dir.join("b/ruby/compat")).unwrap();
        std::fs::write(dir.join("a/ruby/compat/foo.rb"), "# a\n").unwrap();
        std::fs::write(dir.join("b/ruby/compat/foo.rb"), "# b\n").unwrap();
        let generation = Generation::new(dir.join("Cargo.toml")).ruby_extra_files(vec![
            PathBuf::from("a/ruby/compat/foo.rb"),
            PathBuf::from("b/ruby/compat/foo.rb"),
        ]);
        let error = generation
            .validate_ruby_extra_files()
            .unwrap_err()
            .to_string();
        assert!(error.contains("duplicate destination path"));
        std::fs::remove_dir_all(dir).ok();
    }
}
