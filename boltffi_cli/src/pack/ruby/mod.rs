//! `boltffi pack ruby`: generated gem sources plus the host static library.
//!
//! The pack generates the sources and builds the Rust static library from
//! one binding expansion, copies the library into the generated extension
//! directory, and writes its link metadata beside it. `gem build` in that
//! tree then produces an installable gem.

use std::path::{Path, PathBuf};

use boltffi_backend::target::ruby::name_style::extension_stem;
use boltffi_bindgen::target::Target;

use crate::{
    build::{
        BindingExpansion, BuildOptions, BuildSelection, Builder, CargoBuildProfile, OutputCallback,
        native_link::NativeLinkMetadata, resolve_build_profile,
    },
    cargo::Cargo,
    cli::{CliError, Result},
    commands::{generate::bindings::render_ruby, pack::PackRubyOptions},
    config::{Config, TargetSection},
    pack::{apple::compute_checksum, print_cargo_line, resolve_build_cargo_args},
    reporter::Reporter,
    target::NativeHostPlatform,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct RubyLinkMetadata {
    #[serde(flatten)]
    native_link: NativeLinkMetadata,
    source_sha256: String,
}

pub(crate) fn pack_ruby(
    config: &Config,
    options: PackRubyOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_ruby_enabled() {
        return Err(failed("targets.ruby.enabled = false"));
    }
    if !config.should_process(Target::Ruby, options.experimental) {
        return Err(failed(
            "ruby is experimental, use --experimental or add \"ruby\" to experimental",
        ));
    }
    let platform = supported_host(NativeHostPlatform::current())?;

    reporter.section("💎", "Packing Ruby");
    if !options.execution.regenerate && !options.execution.no_build {
        return Err(failed(
            "pack ruby builds the static library and the sources together; remove '--regenerate false'",
        ));
    }

    let build_cargo_args =
        resolve_build_cargo_args(config, TargetSection::Ruby, &options.execution.cargo_args);
    let cargo = Cargo::current(&build_cargo_args)?;
    if let Some(target) = cargo
        .target_selector()
        .map(str::to_owned)
        .or_else(|| cargo.configured_build_target())
    {
        return Err(failed(&format!(
            "pack ruby is host-only; remove cargo target '{target}'"
        )));
    }
    let build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);
    let binding_expansion = BindingExpansion::resolve(config, &build_cargo_args)?;
    if !binding_expansion.selected_library().builds_staticlib() {
        return Err(failed(
            "pack ruby requires the selected Rust library target to build a staticlib",
        ));
    }

    if options.execution.regenerate && !options.execution.no_build {
        // The sources and the static library come from one expansion, so
        // `[cargo.command_args].generate` cannot add a feature the library lacks.
        let step = reporter.step("Generating Ruby bindings");
        render_ruby(
            config,
            &binding_expansion,
            &config.ruby_output(),
            options.execution.deny_skipped,
        )?;
        step.finish_success();
    }

    let artifact_name = binding_expansion.artifact_name().to_owned();
    let static_library = platform.static_library_filename(&artifact_name);
    let profile_dir = binding_expansion
        .target_directory()
        .join(build_profile.output_directory_name());
    let extension_dir = extension_dir(&config.ruby_output(), config.ruby_gem_name())?;
    let vendored = extension_dir.join(&static_library);
    let link_metadata = extension_dir.join(format!("{static_library}.boltffi-link.json"));

    if options.execution.no_build {
        if !vendored.exists() || !link_metadata.exists() {
            return Err(failed(
                "no packed Ruby static library; run pack ruby without --no-build first",
            ));
        }
        validate_link_metadata(&extension_dir, &link_metadata)?;
        reporter.finish();
        return Ok(());
    }

    let step = reporter.step("Building Rust static library");
    let on_output: Option<OutputCallback> = step
        .is_verbose()
        .then(|| Box::new(print_cargo_line) as OutputCallback);
    let native_link = Builder::new(
        config,
        BuildOptions {
            release: matches!(build_profile, CargoBuildProfile::Release),
            selection: BuildSelection::Expanded(Box::new(binding_expansion)),
            on_output,
            extra_env: Vec::new(),
        },
    )
    .build_host_with_native_link_metadata()?;
    step.finish_success();

    let step = reporter.step("Vendoring static library into the gem");
    let built = profile_dir.join(&static_library);
    std::fs::copy(&built, &vendored).map_err(|source| CliError::CopyFailed {
        from: built,
        to: vendored.clone(),
        source,
    })?;
    write_link_metadata(native_link, &extension_dir, &link_metadata)?;
    step.finish_success();
    reporter.finish();
    Ok(())
}

/// Returns the host when `pack ruby` can build on it.
///
/// `extconf.rb` links a Unix `lib<name>.a` archive, so the pack refuses a
/// Windows host before it generates or builds anything.
fn supported_host(host: Option<NativeHostPlatform>) -> Result<NativeHostPlatform> {
    match host {
        Some(NativeHostPlatform::WindowsX86_64 | NativeHostPlatform::WindowsAarch64) | None => {
            Err(failed("pack ruby supports only macOS and Linux hosts"))
        }
        Some(host) => Ok(host),
    }
}

/// Writes the link metadata with a digest of the extension source.
///
/// `extconf.rb` refuses the vendored library when the source changed after
/// the pack, such as by a later `boltffi generate ruby`. The library could
/// lack a function that the new source calls, and Ruby would crash at the
/// first call instead of failing the build.
fn write_link_metadata(
    native_link: NativeLinkMetadata,
    extension_dir: &Path,
    path: &Path,
) -> Result<()> {
    let metadata = RubyLinkMetadata {
        native_link,
        source_sha256: source_checksum(extension_dir)?,
    };
    let bytes = serde_json::to_vec(&metadata)
        .map_err(|source| failed(&format!("serialize Ruby link metadata: {source}")))?;
    std::fs::write(path, bytes).map_err(|source| CliError::WriteFailed {
        path: path.to_path_buf(),
        source,
    })
}

fn source_checksum(extension_dir: &Path) -> Result<String> {
    let stem = extension_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| failed("the Ruby extension directory has no UTF-8 name"))?;
    compute_checksum(&extension_dir.join(format!("{stem}.c")))
}

fn validate_link_metadata(extension_dir: &Path, path: &Path) -> Result<()> {
    let bytes = std::fs::read(path).map_err(|source| CliError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;
    let metadata: RubyLinkMetadata = serde_json::from_slice(&bytes).map_err(|source| {
        failed(&format!(
            "invalid Ruby link metadata {}: {source}; run pack ruby without --no-build first",
            path.display()
        ))
    })?;
    if metadata.source_sha256 != source_checksum(extension_dir)? {
        return Err(failed(&format!(
            "packed Ruby library metadata {} belongs to other extension sources; run pack ruby without --no-build first",
            path.display()
        )));
    }
    Ok(())
}

/// Returns the generated extension directory for the current gem.
fn extension_dir(output: &Path, gem: &str) -> Result<PathBuf> {
    let directory = output.join("ext").join(extension_stem(gem));
    if !directory.join("extconf.rb").is_file() {
        return Err(failed(&format!(
            "no generated Ruby extension at {}; run generate ruby first",
            directory.display()
        )));
    }
    Ok(directory)
}

fn failed(command: &str) -> CliError {
    CliError::CommandFailed {
        command: command.to_owned(),
        status: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_ruby_refuses_hosts_that_extconf_cannot_link() {
        use NativeHostPlatform::*;

        for host in [Some(WindowsX86_64), Some(WindowsAarch64), None] {
            assert!(supported_host(host).is_err(), "{host:?}");
        }
        for host in [DarwinArm64, DarwinX86_64, LinuxX86_64, LinuxAarch64] {
            assert_eq!(supported_host(Some(host)).ok(), Some(host));
        }
    }

    fn native_link() -> NativeLinkMetadata {
        NativeLinkMetadata {
            native_static_libraries: vec!["-lc".to_owned()],
            native_link_search_paths: Vec::new(),
        }
    }

    fn packed_extension() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let extension_dir = root.path().join("demo");
        std::fs::create_dir(&extension_dir).unwrap();
        std::fs::write(extension_dir.join("demo.c"), "void Init_demo(void) {}\n").unwrap();
        let metadata = extension_dir.join("libdemo.a.boltffi-link.json");
        write_link_metadata(native_link(), &extension_dir, &metadata).unwrap();
        (root, extension_dir, metadata)
    }

    #[test]
    fn changed_sources_require_repacking() {
        let (_root, extension_dir, metadata) = packed_extension();
        validate_link_metadata(&extension_dir, &metadata).unwrap();

        std::fs::write(
            extension_dir.join("demo.c"),
            "void Init_demo(void) { rb_define_module(\"Changed\"); }\n",
        )
        .unwrap();
        assert!(validate_link_metadata(&extension_dir, &metadata).is_err());

        write_link_metadata(native_link(), &extension_dir, &metadata).unwrap();
        validate_link_metadata(&extension_dir, &metadata).unwrap();
    }

    #[test]
    fn unusable_metadata_cannot_be_reused() {
        let (_root, extension_dir, metadata) = packed_extension();
        std::fs::write(&metadata, b"{").unwrap();
        assert!(validate_link_metadata(&extension_dir, &metadata).is_err());

        let missing_digest = serde_json::json!({
            "native_static_libraries": ["-lc"],
            "native_link_search_paths": [],
        });
        std::fs::write(&metadata, missing_digest.to_string()).unwrap();
        assert!(validate_link_metadata(&extension_dir, &metadata).is_err());
    }
}
