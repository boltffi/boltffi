//! C package assembler.
//!
//! The C package is intentionally the simplest in the repository: the
//! ergonomic header, a shared library, and a static archive. It is build-system
//! agnostic so downstream users can easily integrate those artifacts into their
//! own build system.
//!
//! ```text
//! dist/c/
//! ├── include/<library>.h
//! └── lib/
//!     ├── lib<library>.{so,dylib,dll}
//!     └── lib<library>.a
//! ```
//!
//! A consumer links with `-I dist/c/include -L dist/c/lib -l<library>` and
//! chooses dynamic or static linking by picking `lib<library>.so` or
//! `lib<library>.a`.

use std::path::PathBuf;
use std::process::Command;

use crate::{
    build::{BindingExpansion, CargoBuildProfile, resolve_build_profile},
    cargo::Cargo,
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::PackCOptions,
    },
    config::Config,
    pack::resolve_build_cargo_args,
    reporter::Reporter,
    target::NativeHostPlatform,
};

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

/// Configures the same binding-expansion ABI shim build used by the other
/// native packers.
fn host_libraries_command(expansion: &BindingExpansion, release: bool) -> Result<Command> {
    let mut command = Command::new("cargo");
    if let Some(toolchain_selector) = expansion.toolchain_selector() {
        command.arg(toolchain_selector);
    }
    command
        .arg("rustc")
        .arg("--manifest-path")
        .arg(expansion.cargo_manifest_path())
        .arg("-p")
        .arg(expansion.package_id());
    if release {
        command.arg("--release");
    }
    command.args(expansion.cargo_args());
    command.arg("--lib");
    expansion.configure_rustc(&mut command)?;
    Ok(command)
}

/// Builds the same binding-expansion ABI shim used by the other native packers.
fn build_host_libraries(expansion: &BindingExpansion, release: bool) -> Result<()> {
    let mut command = host_libraries_command(expansion, release)?;
    let status = command.status().map_err(|source| CliError::CommandFailed {
        command: format!("cargo rustc: {source}"),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: "cargo rustc".to_string(),
            status: status.code(),
        });
    }
    Ok(())
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

    reporter.section("🌐", "Packing C");

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let cargo = Cargo::current(&build_cargo_args)?;
    ensure_c_pack_cargo_args_supported(&cargo)?;
    let build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);
    let binding_expansion = BindingExpansion::resolve(config, &build_cargo_args)?;
    ensure_c_library_outputs(&binding_expansion)?;

    // Generating the header runs a metadata rustc invocation. Build the final
    // artifacts afterwards so that metadata compilation cannot overwrite the
    // binding-expansion cdylib/staticlib selected for this package.
    if options.execution.regenerate {
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

    if !options.execution.no_build {
        let step = reporter.step("Building Rust shared and static libraries");
        build_host_libraries(
            &binding_expansion,
            matches!(build_profile, CargoBuildProfile::Release),
        )?;
        step.finish_success();
    }

    let step = reporter.step("Packaging header and libraries");

    let output_dir = config.c_output();
    let include_dir = output_dir.join("include");
    let lib_dir = output_dir.join("lib");
    std::fs::create_dir_all(&include_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: include_dir.clone(),
        source,
    })?;
    std::fs::create_dir_all(&lib_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: lib_dir.clone(),
        source,
    })?;

    let library_name = config.library_name().to_string();
    let artifact_name = binding_expansion.artifact_name();
    let target_directory = binding_expansion.target_directory();
    let profile_dir = target_directory.join(build_profile.output_directory_name());

    // Header.
    let header_source = output_dir.join("boltffi.h");
    let header_dest = include_dir.join(format!("{library_name}.h"));
    copy_file(header_source, header_dest)?;

    let platform = NativeHostPlatform::current().ok_or_else(|| CliError::CommandFailed {
        command: "pack c is unsupported on this host platform".to_owned(),
        status: None,
    })?;

    // Shared library.
    copy_file(
        profile_dir.join(platform.shared_library_filename(artifact_name)),
        lib_dir.join(platform.shared_library_filename(&library_name)),
    )?;

    // Static archive.
    copy_file(
        profile_dir.join(platform.static_library_filename(artifact_name)),
        lib_dir.join(platform.static_library_filename(&library_name)),
    )?;

    step.finish_success();
    reporter.finish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_c_library_outputs, host_libraries_command};
    use crate::build::BindingExpansion;

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

    #[test]
    fn c_build_uses_the_binding_expansion_abi_shim() {
        let expansion = BindingExpansion::fixture(
            "/external/workspace/Cargo.toml",
            "/external/workspace/demo/Cargo.toml",
            ["--features".to_string(), "ffi".to_string()],
        );
        let package_id = expansion.package_id().to_owned();
        let command = host_libraries_command(&expansion, true).expect("command");
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(arguments.first().map(String::as_str), Some("+nightly"));
        assert_eq!(arguments.get(1).map(String::as_str), Some("rustc"));
        assert!(arguments.windows(2).any(|arguments| {
            arguments == ["--manifest-path", "/external/workspace/Cargo.toml"]
        }));
        assert!(
            arguments
                .windows(2)
                .any(|arguments| arguments == ["-p", package_id.as_str()])
        );
        assert_eq!(
            &arguments[arguments.len() - 4..],
            ["--lib", "--", "--cfg", "boltffi_binding_expansion"]
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| { key == "BOLTFFI_BINDING_EXPANSION" && value.is_some() })
        );
    }
}
