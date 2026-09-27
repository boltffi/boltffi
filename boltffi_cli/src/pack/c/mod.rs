mod package;

use std::path::PathBuf;

use boltffi_bindgen::target::Target;

use crate::{
    build::{
        BindingExpansion, BuildOptions, BuildSelection, Builder, CargoBuildProfile, OutputCallback,
        resolve_build_profile,
    },
    cargo::Cargo,
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::PackCOptions,
    },
    config::Config,
    pack::{print_cargo_line, resolve_build_cargo_args},
    reporter::Reporter,
    target::NativeHostPlatform,
};

use self::package::CPackage;
use crate::build::native_link::NativeLinkMetadata;

fn ensure_c_pack_cargo_args_supported(cargo: &Cargo) -> Result<()> {
    if let Some(target) = cargo.target_selector() {
        return Err(CliError::CommandFailed {
            command: format!("pack c is host-only; remove cargo --target '{target}'"),
            status: None,
        });
    }
    if let Some(target) = cargo.configured_build_target() {
        return Err(CliError::CommandFailed {
            command: format!("pack c is host-only; remove cargo build.target '{target}'"),
            status: None,
        });
    }
    Ok(())
}

fn ensure_c_library_outputs(expansion: &BindingExpansion) -> Result<()> {
    let library = expansion.selected_library();
    if library.builds_cdylib() && library.builds_staticlib() {
        return Ok(());
    }
    Err(CliError::CommandFailed {
        command:
            "pack c requires the selected Rust library target to build both cdylib and staticlib"
                .to_owned(),
        status: None,
    })
}

fn copy_file(source: PathBuf, dest: PathBuf) -> Result<()> {
    std::fs::copy(&source, &dest)
        .map(|_| ())
        .map_err(|source_err| CliError::CopyFailed {
            from: source,
            to: dest,
            source: source_err,
        })
}

pub(crate) fn pack_c(config: &Config, options: PackCOptions, reporter: &Reporter) -> Result<()> {
    if !config.is_c_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.c.enabled = false".to_string(),
            status: None,
        });
    }
    if !config.should_process(Target::C, options.experimental) {
        return Err(CliError::CommandFailed {
            command: "c is experimental, use --experimental or add \"c\" to experimental"
                .to_owned(),
            status: None,
        });
    }

    reporter.section("🌐", "Packing C");

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let cargo = Cargo::current(&build_cargo_args)?;
    ensure_c_pack_cargo_args_supported(&cargo)?;
    let build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);
    let binding_expansion = BindingExpansion::resolve(config, &build_cargo_args)?;
    ensure_c_library_outputs(&binding_expansion)?;

    if options.execution.regenerate && !options.execution.no_build {
        let step = reporter.step("Generating C bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::C,
                output: Some(config.c_output()),
                experimental: options.experimental,
                cargo_args: build_cargo_args.clone(),
                deny_skipped: options.execution.deny_skipped,
            },
        )?;
        step.finish_success();
    }

    let platform = NativeHostPlatform::current().ok_or_else(|| CliError::CommandFailed {
        command: "pack c is unsupported on this host platform".to_owned(),
        status: None,
    })?;
    let package = CPackage::new(config, binding_expansion.selected_library(), platform)?;
    let artifact_name = binding_expansion.artifact_name().to_owned();
    let profile_dir = binding_expansion
        .target_directory()
        .join(build_profile.output_directory_name());

    let link_metadata_path = profile_dir.join(format!("{artifact_name}.boltffi-link.json"));
    let native_link = if options.execution.no_build {
        let metadata = NativeLinkMetadata::read(&link_metadata_path).map_err(|error| {
            CliError::CommandFailed {
                command: format!("{error}; run pack c without --no-build first"),
                status: None,
            }
        })?;
        let metadata_time = std::fs::metadata(&link_metadata_path)
            .and_then(|metadata| metadata.modified())
            .map_err(|source| CliError::ReadFailed {
                path: link_metadata_path.clone(),
                source,
            })?;
        [
            platform.static_library_filename(&artifact_name),
            platform.shared_library_filename(&artifact_name),
        ]
        .iter()
        .try_for_each(|filename| {
            let path = profile_dir.join(filename);
            let modified = std::fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .map_err(|source| CliError::ReadFailed { path, source })?;
            if modified > metadata_time {
                return Err(CliError::CommandFailed {
                    command:
                        "native libraries changed since C packaging; run pack c without --no-build"
                            .to_owned(),
                    status: None,
                });
            }
            Ok(())
        })?;
        metadata
    } else {
        let step = reporter.step("Building Rust shared and static libraries");
        let on_output: Option<OutputCallback> = step
            .is_verbose()
            .then(|| Box::new(print_cargo_line) as OutputCallback);
        let builder = Builder::new(
            config,
            BuildOptions {
                release: matches!(build_profile, CargoBuildProfile::Release),
                selection: BuildSelection::Expanded(Box::new(binding_expansion)),
                on_output,
                extra_env: Vec::new(),
            },
        );
        let native_link = builder.build_host_with_native_link_metadata()?;
        native_link.write(&link_metadata_path)?;
        step.finish_success();
        native_link
    };

    let step = reporter.step("Packaging header, libraries, and build metadata");

    let output_dir = config.c_output();
    let include_dir = output_dir.join("include");
    let lib_dir = output_dir.join("lib");
    let static_lib_dir = lib_dir.join("static");
    [&include_dir, &static_lib_dir]
        .into_iter()
        .try_for_each(|directory| {
            std::fs::create_dir_all(directory).map_err(|source| CliError::CreateDirectoryFailed {
                path: directory.to_path_buf(),
                source,
            })
        })?;

    let library_name = config.library_name().to_string();

    let header_source = output_dir.join("boltffi.h");
    let header_dest = include_dir.join(format!("{library_name}.h"));
    copy_file(header_source, header_dest)?;

    copy_file(
        profile_dir.join(platform.shared_library_filename(&artifact_name)),
        lib_dir.join(platform.shared_library_filename(&artifact_name)),
    )?;

    copy_file(
        profile_dir.join(platform.static_library_filename(&artifact_name)),
        static_lib_dir.join(platform.static_library_filename(&artifact_name)),
    )?;

    let previous_archive = lib_dir.join(platform.static_library_filename(&artifact_name));
    match std::fs::remove_file(&previous_archive) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliError::CommandFailed {
                command: format!(
                    "remove old C package archive {}: {error}",
                    previous_archive.display()
                ),
                status: None,
            });
        }
    }

    if let Some(filename) = platform.import_library_filename(&artifact_name) {
        copy_file(profile_dir.join(&filename), lib_dir.join(filename))?;
    }

    package.relocate_shared_library(&output_dir)?;
    package.write_metadata(&output_dir, &native_link)?;

    step.finish_success();
    reporter.finish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_c_library_outputs, pack_c};
    use crate::build::BindingExpansion;
    use crate::commands::pack::{PackCOptions, PackExecutionOptions};
    use crate::config::Config;
    use crate::reporter::{Reporter, Verbosity};

    #[test]
    fn c_pack_requires_opt_in_even_without_regeneration_or_build() {
        let config: Config =
            toml::from_str("[package]\nname = \"demo\"\n[targets.c]\nenabled = true\n")
                .expect("config");
        let options = PackCOptions {
            execution: PackExecutionOptions {
                release: false,
                regenerate: false,
                no_build: true,
                deny_skipped: false,
                cargo_args: Vec::new(),
            },
            experimental: false,
        };
        let error = pack_c(&config, options, &Reporter::new(Verbosity::Quiet))
            .expect_err("C packaging requires opt-in before any Cargo work");
        assert!(error.to_string().contains("c is experimental"));
    }

    #[test]
    fn c_pack_requires_shared_and_static_library_outputs() {
        for (staticlib, cdylib) in [(false, true), (true, false), (false, false)] {
            let expansion = BindingExpansion::fixture(
                "/external/workspace/Cargo.toml",
                "/external/workspace/demo/Cargo.toml",
                std::iter::empty(),
            )
            .fixture_outputs(staticlib, cdylib);
            let error = ensure_c_library_outputs(&expansion).expect_err("missing output rejects");
            assert!(format!("{error}").contains("both cdylib and staticlib"));
        }
    }
}
