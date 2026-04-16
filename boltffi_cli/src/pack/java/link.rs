use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::build::{CargoBuildCommand, OutputCallback, run_command_streaming};
use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::pack::PackError;
use crate::pack::{format_command_for_log, print_cargo_line, print_verbose_detail};
use crate::reporter::Step;
use crate::target::JavaHostTarget;

use super::plan::{JvmCargoContext, JvmCrateOutputs, JvmPackagingTarget};

pub(crate) struct JvmBuildArtifacts {
    pub(crate) native_static_libraries: Vec<String>,
    pub(crate) native_link_search_paths: Vec<String>,
    pub(crate) static_library_filename: Option<String>,
}

pub(crate) struct JniLinkerArgs<'a> {
    pub(crate) host_target: JavaHostTarget,
    pub(crate) output_lib: &'a Path,
    pub(crate) jni_glue: &'a Path,
    pub(crate) link_input: &'a Path,
    pub(crate) jni_dir: &'a Path,
    pub(crate) jni_include_directories: &'a JniIncludeDirectories,
    pub(crate) rustflag_linker_args: &'a [String],
    pub(crate) native_link_search_paths: &'a [String],
    pub(crate) native_static_libraries: &'a [String],
    pub(crate) rpath_flag: Option<&'a str>,
}

pub(crate) struct NativeLinkMetadata {
    pub(crate) native_static_libraries: Vec<String>,
    pub(crate) native_link_search_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct JvmPackagedNativeOutput {
    pub(crate) host_target: JavaHostTarget,
    pub(crate) has_shared_library_copy: bool,
}

#[derive(Debug)]
pub(crate) struct JniIncludeDirectories {
    pub(crate) shared: PathBuf,
    pub(crate) platform: PathBuf,
}

pub(crate) enum JvmNativeLinkInput {
    Staticlib(PathBuf),
    Cdylib(PathBuf),
}

impl JvmNativeLinkInput {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Staticlib(path) | Self::Cdylib(path) => path,
        }
    }

    fn links_staticlib(&self) -> bool {
        matches!(self, Self::Staticlib(_))
    }
}

impl JvmBuildArtifacts {
    fn static_library_filename(&self) -> Option<&str> {
        self.static_library_filename.as_deref()
    }
}

pub(crate) fn compile_jni_library(
    config: &Config,
    packaging_target: &JvmPackagingTarget,
    build_artifacts: &JvmBuildArtifacts,
    step: &Step,
) -> Result<JvmPackagedNativeOutput> {
    let cargo_context = &packaging_target.cargo_context;
    let host_target = cargo_context.host_target;
    let java_output_raw = config.java_jvm_output();
    // Ensure all derived paths are absolute so they work inside containers.
    let java_output = if java_output_raw.is_absolute() {
        java_output_raw
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(java_output_raw)
    };
    let jni_dir = java_output.join("jni");
    let jni_glue = jni_dir.join("jni_glue.c");
    let header = jni_dir.join(format!("{}.h", cargo_context.artifact_name));

    if !jni_glue.exists() {
        return Err(CliError::FileNotFound(jni_glue));
    }
    if !header.exists() {
        return Err(CliError::FileNotFound(header));
    }

    let artifact_name = &cargo_context.artifact_name;
    let link_input = resolve_jvm_native_link_input(
        &cargo_context.artifact_directory(),
        host_target,
        artifact_name,
        cargo_context.crate_outputs,
        build_artifacts.static_library_filename(),
    )?;
    let compatibility_shared_library = bundled_jvm_shared_library_path(
        &link_input,
        &cargo_context.artifact_directory(),
        host_target,
        artifact_name,
        cargo_context.crate_outputs,
    );

    let host_native_output = java_output
        .join("native")
        .join(host_target.canonical_name());
    std::fs::create_dir_all(&host_native_output).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: host_native_output.clone(),
            source,
        }
    })?;

    let output_lib = host_native_output.join(host_target.jni_library_filename(artifact_name));
    let jni_include_directories = resolve_jni_include_directories(cargo_context)?;
    let has_shared_library_copy = compatibility_shared_library.is_some();

    let volume_paths: Vec<&Path> = vec![
        output_lib.as_path(),
        jni_glue.as_path(),
        link_input.path(),
        jni_dir.as_path(),
        jni_include_directories.shared.as_path(),
        jni_include_directories.platform.as_path(),
    ];
    let mut command = packaging_target
        .toolchain
        .container_linker_command(&volume_paths);
    let jni_linker_args = if packaging_target.toolchain.uses_msvc_compiler() {
        clang_cl_jni_linker_args(&JniLinkerArgs {
            host_target,
            output_lib: &output_lib,
            jni_glue: &jni_glue,
            link_input: link_input.path(),
            jni_dir: &jni_dir,
            jni_include_directories: &jni_include_directories,
            rustflag_linker_args: packaging_target.toolchain.jni_rustflag_linker_args(),
            native_link_search_paths: &build_artifacts.native_link_search_paths,
            native_static_libraries: &build_artifacts.native_static_libraries,
            rpath_flag: None,
        })?
    } else {
        clang_style_jni_linker_args(&JniLinkerArgs {
            host_target,
            output_lib: &output_lib,
            jni_glue: &jni_glue,
            link_input: link_input.path(),
            jni_dir: &jni_dir,
            jni_include_directories: &jni_include_directories,
            rustflag_linker_args: packaging_target.toolchain.jni_rustflag_linker_args(),
            native_link_search_paths: &build_artifacts.native_link_search_paths,
            native_static_libraries: &build_artifacts.native_static_libraries,
            rpath_flag: host_target.rpath_flag(),
        })
    };
    command.args(jni_linker_args);

    if step.is_verbose() {
        print_verbose_detail(&format!(
            "JNI rustflag linker args: {:?}",
            packaging_target.toolchain.jni_rustflag_linker_args()
        ));
        print_verbose_detail(&format!(
            "JNI native link search paths: {:?}",
            &build_artifacts.native_link_search_paths
        ));
        print_verbose_detail(&format!(
            "JNI native static libs: {:?}",
            &build_artifacts.native_static_libraries
        ));
        print_verbose_detail(&format!(
            "JNI linker command: {}",
            format_command_for_log(&command)
        ));
    }

    let status = command.status().map_err(|source| CliError::CommandFailed {
        command: format!("desktop linker: {}", source),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: format!(
                "desktop linker failed to compile JNI library for '{}'",
                host_target.canonical_name()
            ),
            status: status.code(),
        });
    }

    let current_host = JavaHostTarget::current();
    if current_host == Some(host_target) {
        let compatibility_jni_copy =
            java_output.join(host_target.jni_library_filename(artifact_name));
        std::fs::copy(&output_lib, &compatibility_jni_copy).map_err(|source| {
            CliError::CopyFailed {
                from: output_lib.clone(),
                to: compatibility_jni_copy,
                source,
            }
        })?;
    }

    if let Some(shared_library) = compatibility_shared_library.as_deref() {
        let shared_library_name = shared_library
            .file_name()
            .expect("shared library path should have a file name");
        let structured_copy = host_native_output.join(shared_library_name);
        std::fs::copy(shared_library, &structured_copy).map_err(|source| CliError::CopyFailed {
            from: shared_library.to_path_buf(),
            to: structured_copy,
            source,
        })?;

        if current_host == Some(host_target) {
            let flat_copy = java_output.join(shared_library_name);
            std::fs::copy(shared_library, &flat_copy).map_err(|source| CliError::CopyFailed {
                from: shared_library.to_path_buf(),
                to: flat_copy,
                source,
            })?;
        }
    }

    Ok(JvmPackagedNativeOutput {
        host_target,
        has_shared_library_copy,
    })
}

pub(crate) fn build_jvm_native_library(
    packaging_target: &JvmPackagingTarget,
    release: bool,
    step: &Step,
) -> Result<JvmBuildArtifacts> {
    let cargo_context = &packaging_target.cargo_context;
    let native_static_libraries = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured_static_libraries = Arc::clone(&native_static_libraries);
    let verbose = step.is_verbose();
    let on_output: Option<OutputCallback> = Some(Box::new(move |line: &str| {
        if verbose {
            print_cargo_line(line);
        }

        if let Some(flags) = parse_native_static_libraries(line) {
            let mut libraries = captured_static_libraries
                .lock()
                .expect("native static libraries lock poisoned");
            *libraries = flags;
        }
    }));

    let crate_directory = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;
    let cmd = cargo_context.cargo_build_command.as_ref();
    let program = cmd.map(CargoBuildCommand::program).unwrap_or("cargo");
    let subcommand = cmd.map(CargoBuildCommand::subcommand).unwrap_or("build");
    let is_cargo = cmd.map(CargoBuildCommand::is_cargo_program).unwrap_or(true);
    let mut command = Command::new(program);
    command.current_dir(crate_directory);

    if is_cargo && let Some(toolchain_selector) = cargo_context.toolchain_selector.as_deref() {
        command.arg(toolchain_selector);
    }

    command
        .arg(subcommand)
        .arg("--target")
        .arg(cargo_context.cargo_target_arg());
    apply_jvm_cargo_package_selection(&mut command, cargo_context);

    if release {
        command.arg("--release");
    }

    command.args(&cargo_context.cargo_command_args);
    packaging_target
        .toolchain
        .configure_cargo_build(&mut command);

    if step.is_verbose() {
        print_verbose_detail(&format!(
            "cargo build command: {}",
            format_command_for_log(&command)
        ));
    }

    if !run_command_streaming(&mut command, on_output.as_ref()) {
        return Err(PackError::BuildFailed {
            targets: vec![cargo_context.host_target.canonical_name().to_string()],
        }
        .into());
    }

    let native_static_libraries = native_static_libraries
        .lock()
        .expect("native static libraries lock poisoned")
        .clone();
    let mut native_link_search_paths = Vec::new();

    let native_static_libraries = if native_static_libraries.is_empty() {
        let static_library_filename = if cargo_context.crate_outputs.builds_staticlib {
            resolve_static_library_filename(cargo_context)?
        } else {
            None
        };
        let staticlib_path = static_library_filename
            .as_ref()
            .map(|filename| cargo_context.artifact_directory().join(filename));

        if cargo_context.crate_outputs.builds_staticlib
            && staticlib_path
                .as_ref()
                .is_some_and(|staticlib_path| staticlib_path.exists())
        {
            let link_metadata = query_native_link_metadata(packaging_target, release)?;
            native_link_search_paths = link_metadata.native_link_search_paths;
            link_metadata.native_static_libraries
        } else {
            native_static_libraries
        }
    } else {
        let static_library_filename = if cargo_context.crate_outputs.builds_staticlib {
            resolve_static_library_filename(cargo_context)?
        } else {
            None
        };
        let staticlib_path = static_library_filename
            .as_ref()
            .map(|filename| cargo_context.artifact_directory().join(filename));

        if cargo_context.crate_outputs.builds_staticlib
            && staticlib_path
                .as_ref()
                .is_some_and(|staticlib_path| staticlib_path.exists())
        {
            native_link_search_paths =
                query_native_link_metadata(packaging_target, release)?.native_link_search_paths;
        }

        native_static_libraries
    };

    let static_library_filename = if cargo_context.crate_outputs.builds_staticlib {
        resolve_static_library_filename(cargo_context)?
    } else {
        None
    };

    Ok(JvmBuildArtifacts {
        native_static_libraries,
        native_link_search_paths,
        static_library_filename,
    })
}

pub(crate) fn resolve_jvm_native_link_input(
    artifact_directory: &Path,
    host_target: JavaHostTarget,
    artifact_name: &str,
    crate_outputs: JvmCrateOutputs,
    static_library_filename: Option<&str>,
) -> Result<JvmNativeLinkInput> {
    let staticlib_path = static_library_filename.map(|filename| artifact_directory.join(filename));
    if crate_outputs.builds_staticlib
        && staticlib_path
            .as_ref()
            .is_some_and(|staticlib_path| staticlib_path.exists())
    {
        return Ok(JvmNativeLinkInput::Staticlib(
            staticlib_path.expect("checked staticlib path existence"),
        ));
    }

    let cdylib_path = artifact_directory.join(host_target.shared_library_filename(artifact_name));
    if crate_outputs.builds_cdylib && cdylib_path.exists() {
        return Ok(JvmNativeLinkInput::Cdylib(cdylib_path));
    }

    if crate_outputs.builds_staticlib {
        return Err(CliError::FileNotFound(staticlib_path.unwrap_or_else(
            || artifact_directory.join(host_target.static_library_filename(artifact_name)),
        )));
    }

    if crate_outputs.builds_cdylib {
        return Err(CliError::FileNotFound(cdylib_path));
    }

    Err(CliError::CommandFailed {
        command:
            "the current library target must enable either staticlib or cdylib for JVM packaging"
                .to_string(),
        status: None,
    })
}

pub(crate) fn existing_jvm_shared_library_path(
    artifact_directory: &Path,
    host_target: JavaHostTarget,
    artifact_name: &str,
    crate_outputs: JvmCrateOutputs,
) -> Option<PathBuf> {
    if !crate_outputs.builds_cdylib {
        return None;
    }

    let shared_library_path =
        artifact_directory.join(host_target.shared_library_filename(artifact_name));
    shared_library_path.exists().then_some(shared_library_path)
}

pub(crate) fn bundled_jvm_shared_library_path(
    link_input: &JvmNativeLinkInput,
    artifact_directory: &Path,
    host_target: JavaHostTarget,
    artifact_name: &str,
    crate_outputs: JvmCrateOutputs,
) -> Option<PathBuf> {
    if link_input.links_staticlib() {
        return None;
    }

    existing_jvm_shared_library_path(
        artifact_directory,
        host_target,
        artifact_name,
        crate_outputs,
    )
}

pub(crate) fn parse_native_static_libraries(line: &str) -> Option<Vec<String>> {
    let sanitized = strip_ansi_escape_codes(line);
    let (_, flags) = sanitized.split_once("native-static-libs:")?;
    let parsed: Vec<String> = flags
        .split_whitespace()
        .map(str::to_string)
        .filter(|flag| !flag.is_empty())
        .collect();

    (!parsed.is_empty()).then_some(parsed)
}

pub(crate) fn extract_library_filenames(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.contains(' ')
                && [".a", ".lib", ".dylib", ".so", ".rlib", ".dll"]
                    .iter()
                    .any(|extension| line.ends_with(extension))
        })
        .map(str::to_string)
        .collect()
}

pub(crate) fn select_windows_static_library_filename(
    artifact_name: &str,
    filenames: &[String],
) -> Option<String> {
    let msvc_name = format!("{artifact_name}.lib");
    let gnu_name = format!("lib{artifact_name}.a");

    filenames
        .iter()
        .find(|filename| *filename == &msvc_name || *filename == &gnu_name)
        .cloned()
}

pub(crate) fn extract_native_static_libraries(output: &str) -> Option<Vec<String>> {
    output
        .lines()
        .filter_map(parse_native_static_libraries)
        .next_back()
}

pub(crate) fn extract_link_search_paths(output: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct BuildScriptExecutedMessage {
        reason: String,
        #[serde(default)]
        linked_paths: Vec<String>,
    }

    let mut linked_paths = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
    {
        let Ok(message) = serde_json::from_str::<BuildScriptExecutedMessage>(line) else {
            continue;
        };

        if message.reason != "build-script-executed" {
            continue;
        }

        for linked_path in message.linked_paths {
            if !linked_paths.contains(&linked_path) {
                linked_paths.push(linked_path);
            }
        }
    }

    linked_paths
}

pub(crate) fn link_search_path_flags(link_search_paths: &[String]) -> Vec<String> {
    let mut flags = Vec::new();

    for linked_path in link_search_paths {
        let flag = if let Some(path) = linked_path.strip_prefix("framework=") {
            format!("-F{path}")
        } else if let Some(path) = linked_path.strip_prefix("native=") {
            format!("-L{path}")
        } else if let Some(path) = linked_path.strip_prefix("dependency=") {
            format!("-L{path}")
        } else if let Some(path) = linked_path.strip_prefix("all=") {
            format!("-L{path}")
        } else if let Some(path) = linked_path.strip_prefix("crate=") {
            format!("-L{path}")
        } else {
            format!("-L{linked_path}")
        };

        if !flags.contains(&flag) {
            flags.push(flag);
        }
    }

    flags
}

pub(crate) fn clang_native_static_library_flags(
    host_target: JavaHostTarget,
    native_static_libraries: &[String],
) -> Vec<String> {
    let strip_implicit_darwin_libraries = matches!(
        host_target,
        JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64
    );
    let mut flags = Vec::new();
    let mut index = 0;

    while index < native_static_libraries.len() {
        let flag = &native_static_libraries[index];

        if strip_implicit_darwin_libraries && flag == "-l" {
            if let Some(value) = native_static_libraries.get(index + 1)
                && matches!(value.as_str(), "c" | "m" | "System")
            {
                index += 2;
                continue;
            }
        } else if strip_implicit_darwin_libraries
            && matches!(flag.as_str(), "-lc" | "-lm" | "-lSystem")
        {
            index += 1;
            continue;
        }

        flags.push(flag.clone());
        index += 1;
    }

    flags
}

pub(crate) fn clang_style_jni_linker_args(args: &JniLinkerArgs<'_>) -> Vec<String> {
    let mut resolved_args = vec![
        "-shared".to_string(),
        "-fPIC".to_string(),
        "-o".to_string(),
        args.output_lib.display().to_string(),
        // Force C language mode so the JNI glue compiles correctly even when the
        // compiler driver is a C++ frontend (g++, c++, clang++). The generated
        // code uses the C JNI API ((*env)->Method(env, ...)) and relies on
        // implicit void* conversions that are only valid in C.
        "-x".to_string(),
        "c".to_string(),
        args.jni_glue.display().to_string(),
        // Reset to auto-detection for subsequent inputs (the static library).
        "-x".to_string(),
        "none".to_string(),
        args.link_input.display().to_string(),
        format!("-I{}", args.jni_dir.display()),
        format!("-I{}", args.jni_include_directories.shared.display()),
        format!("-I{}", args.jni_include_directories.platform.display()),
    ];
    resolved_args.extend(args.rustflag_linker_args.iter().cloned());
    resolved_args.extend(link_search_path_flags(args.native_link_search_paths));
    resolved_args.extend(clang_native_static_library_flags(
        args.host_target,
        args.native_static_libraries,
    ));
    if let Some(rpath_flag) = args.rpath_flag {
        resolved_args.push(rpath_flag.to_string());
    }
    resolved_args
}

pub(crate) fn clang_cl_jni_linker_args(args: &JniLinkerArgs<'_>) -> Result<Vec<String>> {
    let mut resolved_args = vec![
        "/LD".to_string(),
        args.jni_glue.display().to_string(),
        args.link_input.display().to_string(),
        format!("/I{}", args.jni_dir.display()),
        format!("/I{}", args.jni_include_directories.shared.display()),
        format!("/I{}", args.jni_include_directories.platform.display()),
        "/link".to_string(),
        format!("/OUT:{}", args.output_lib.display()),
    ];
    resolved_args.extend(msvc_rustflag_linker_args(args.rustflag_linker_args)?);
    resolved_args.extend(msvc_link_search_path_flags(args.native_link_search_paths));
    resolved_args.extend(msvc_native_static_library_flags(
        args.native_static_libraries,
    ));
    Ok(resolved_args)
}

pub(crate) fn msvc_link_search_path_flags(link_search_paths: &[String]) -> Vec<String> {
    let mut flags = Vec::new();

    for linked_path in link_search_paths {
        let Some(path) = linked_path
            .strip_prefix("native=")
            .or_else(|| linked_path.strip_prefix("dependency="))
            .or_else(|| linked_path.strip_prefix("all="))
            .or_else(|| linked_path.strip_prefix("crate="))
            .or_else(|| (!linked_path.starts_with("framework=")).then_some(linked_path.as_str()))
        else {
            continue;
        };

        let flag = format!("/LIBPATH:{path}");
        if !flags.contains(&flag) {
            flags.push(flag);
        }
    }

    flags
}

pub(crate) fn msvc_native_static_library_flags(native_static_libraries: &[String]) -> Vec<String> {
    let mut flags = Vec::new();
    let mut index = 0;

    while index < native_static_libraries.len() {
        let flag = &native_static_libraries[index];

        if flag == "-l" {
            if let Some(value) = native_static_libraries.get(index + 1) {
                flags.push(format!("{value}.lib"));
                index += 2;
                continue;
            }
        } else if let Some(value) = flag.strip_prefix("-l:") {
            flags.push(value.to_string());
            index += 1;
            continue;
        } else if let Some(value) = flag.strip_prefix("-l") {
            if !value.is_empty() {
                flags.push(format!("{value}.lib"));
                index += 1;
                continue;
            }
        } else if flag == "-framework" {
            index += 2;
            continue;
        } else {
            flags.push(flag.clone());
            index += 1;
            continue;
        }

        index += 1;
    }

    flags
}

pub(crate) fn msvc_rustflag_linker_args(rustflag_linker_args: &[String]) -> Result<Vec<String>> {
    let mut flags = Vec::new();
    let mut index = 0;

    while index < rustflag_linker_args.len() {
        let flag = &rustflag_linker_args[index];

        if flag == "-L" {
            if let Some(value) = rustflag_linker_args.get(index + 1) {
                flags.push(format!("/LIBPATH:{value}"));
                index += 2;
                continue;
            }
        } else if let Some(value) = flag.strip_prefix("-L") {
            if !value.is_empty() {
                flags.push(format!("/LIBPATH:{value}"));
                index += 1;
                continue;
            }
        } else if flag == "-l" {
            if let Some(value) = rustflag_linker_args.get(index + 1) {
                flags.push(format!("{value}.lib"));
                index += 2;
                continue;
            }
        } else if let Some(value) = flag.strip_prefix("-l:") {
            flags.push(value.to_string());
            index += 1;
            continue;
        } else if let Some(value) = flag.strip_prefix("-l") {
            if !value.is_empty() {
                flags.push(format!("{value}.lib"));
                index += 1;
                continue;
            }
        } else if flag == "-framework" {
            index += 2;
            continue;
        } else if flag.starts_with("-F") {
            return Err(CliError::CommandFailed {
                command: format!(
                    "unsupported Windows MSVC JNI linker arg '{}' derived from Cargo rustflags",
                    flag
                ),
                status: None,
            });
        } else if flag.starts_with('/') || flag.ends_with(".lib") || flag.ends_with(".a") {
            flags.push(flag.clone());
            index += 1;
            continue;
        } else {
            return Err(CliError::CommandFailed {
                command: format!(
                    "unsupported Windows MSVC JNI linker arg '{}' derived from Cargo rustflags",
                    flag
                ),
                status: None,
            });
        }

        index += 1;
    }

    Ok(flags)
}

pub(crate) fn resolve_jni_include_directories(
    cargo_context: &JvmCargoContext,
) -> Result<JniIncludeDirectories> {
    let java_home_override_env =
        target_specific_java_home_env_key(&cargo_context.rust_target_triple);
    let include_override_env =
        target_specific_java_include_env_key(&cargo_context.rust_target_triple);
    resolve_jni_include_directories_with_overrides(
        cargo_context,
        std::env::var_os("JAVA_HOME").map(PathBuf::from),
        std::env::var_os(&java_home_override_env).map(PathBuf::from),
        std::env::var_os(&include_override_env).map(PathBuf::from),
    )
}

pub(crate) fn resolve_jni_include_directories_with_overrides(
    cargo_context: &JvmCargoContext,
    default_java_home: Option<PathBuf>,
    target_java_home_override: Option<PathBuf>,
    target_include_override: Option<PathBuf>,
) -> Result<JniIncludeDirectories> {
    let java_home_override_env =
        target_specific_java_home_env_key(&cargo_context.rust_target_triple);
    let include_override_env =
        target_specific_java_include_env_key(&cargo_context.rust_target_triple);
    let platform_include = target_include_override.clone().unwrap_or_else(|| {
        target_java_home_override
            .clone()
            .or(default_java_home.clone())
            .map(|java_home| {
                java_home
                    .join("include")
                    .join(cargo_context.host_target.jni_platform())
            })
            .unwrap_or_default()
    });

    let shared_include = target_include_override
        .as_ref()
        .and_then(|platform_include| platform_include.parent().map(Path::to_path_buf))
        .or_else(|| {
            target_java_home_override
                .or(default_java_home)
                .map(|java_home| java_home.join("include"))
        })
        .or_else(|| platform_include.parent().map(Path::to_path_buf))
        .ok_or_else(|| CliError::CommandFailed {
            command: format!(
                "JAVA_HOME not set; for cross-host JVM packaging you can also set {} or {}",
                java_home_override_env, include_override_env
            ),
            status: None,
        })?;

    if !shared_include.exists() {
        return Err(CliError::FileNotFound(shared_include));
    }

    let shared_header = shared_include.join("jni.h");
    if !shared_header.exists() {
        return Err(CliError::FileNotFound(shared_header));
    }

    if !platform_include.exists() {
        return Err(CliError::CommandFailed {
            command: format!(
                "missing JNI platform headers for '{}' at '{}'; set {} to a directory containing jni_md.h or set {} to a target-specific JDK home",
                cargo_context.host_target.canonical_name(),
                platform_include.display(),
                include_override_env,
                java_home_override_env
            ),
            status: None,
        });
    }

    let platform_header = platform_include.join("jni_md.h");
    if !platform_header.exists() {
        return Err(CliError::FileNotFound(platform_header));
    }

    Ok(JniIncludeDirectories {
        shared: shared_include,
        platform: platform_include,
    })
}

pub(crate) fn target_specific_java_home_env_key(rust_target_triple: &str) -> String {
    format!(
        "BOLTFFI_JAVA_HOME_{}",
        rust_target_triple.replace('-', "_").to_uppercase()
    )
}

pub(crate) fn target_specific_java_include_env_key(rust_target_triple: &str) -> String {
    format!(
        "BOLTFFI_JAVA_INCLUDE_{}",
        rust_target_triple.replace('-', "_").to_uppercase()
    )
}

pub(crate) fn query_native_link_metadata(
    packaging_target: &JvmPackagingTarget,
    release: bool,
) -> Result<NativeLinkMetadata> {
    let cargo_context = &packaging_target.cargo_context;
    let crate_directory = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;

    let mut command = Command::new("cargo");
    command.current_dir(crate_directory);

    if let Some(toolchain_selector) = cargo_context.toolchain_selector.as_deref() {
        command.arg(toolchain_selector);
    }

    command
        .arg("rustc")
        .arg("--target")
        .arg(&cargo_context.rust_target_triple);
    apply_jvm_cargo_package_selection(&mut command, cargo_context);

    if release {
        command.arg("--release");
    }

    command
        .args(&cargo_context.cargo_command_args)
        .arg("--message-format=json-render-diagnostics")
        .arg("--lib")
        .arg("--")
        .arg("--print=native-static-libs");
    packaging_target
        .toolchain
        .configure_cargo_build(&mut command);

    let output = command.output().map_err(|source| CliError::CommandFailed {
        command: format!("cargo rustc --print=native-static-libs: {source}"),
        status: None,
    })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: "cargo rustc --print=native-static-libs".to_string(),
            status: output.status.code(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let native_link_search_paths = extract_link_search_paths(&stdout);
    let native_static_libraries =
        extract_native_static_libraries(&combined).ok_or_else(|| CliError::CommandFailed {
            command: "cargo rustc --print=native-static-libs did not emit link metadata"
                .to_string(),
            status: None,
        })?;

    Ok(NativeLinkMetadata {
        native_static_libraries,
        native_link_search_paths,
    })
}

fn strip_ansi_escape_codes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;

            if index >= bytes.len() {
                break;
            }

            if bytes[index] == b'[' {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
                continue;
            }

            continue;
        }

        output.push(bytes[index] as char);
        index += 1;
    }

    output
}

fn resolve_static_library_filename(cargo_context: &JvmCargoContext) -> Result<Option<String>> {
    let artifact_name = &cargo_context.artifact_name;

    if cargo_context.host_target != JavaHostTarget::WindowsX86_64 {
        return Ok(Some(
            cargo_context
                .host_target
                .static_library_filename(artifact_name),
        ));
    }

    let filenames = query_library_filenames(cargo_context)?;
    select_windows_static_library_filename(artifact_name, &filenames)
        .map(Some)
        .ok_or_else(|| CliError::CommandFailed {
            command: format!(
                "cargo rustc --print=file-names did not report a Windows static library for '{}'",
                artifact_name
            ),
            status: None,
        })
}

fn query_library_filenames(cargo_context: &JvmCargoContext) -> Result<Vec<String>> {
    let crate_directory = std::env::current_dir().map_err(|source| CliError::CommandFailed {
        command: format!("current_dir: {source}"),
        status: None,
    })?;

    let mut command = Command::new("cargo");
    command.current_dir(crate_directory);

    if let Some(toolchain_selector) = cargo_context.toolchain_selector.as_deref() {
        command.arg(toolchain_selector);
    }

    command
        .arg("rustc")
        .arg("--target")
        .arg(&cargo_context.rust_target_triple);
    apply_jvm_cargo_package_selection(&mut command, cargo_context);

    if cargo_context.release {
        command.arg("--release");
    }

    command
        .args(&cargo_context.cargo_command_args)
        .arg("--lib")
        .arg("--")
        .arg("--print=file-names");

    let output = command.output().map_err(|source| CliError::CommandFailed {
        command: format!("cargo rustc --print=file-names: {source}"),
        status: None,
    })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: "cargo rustc --print=file-names".to_string(),
            status: output.status.code(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let filenames = extract_library_filenames(&combined);

    if filenames.is_empty() {
        return Err(CliError::CommandFailed {
            command: "cargo rustc --print=file-names did not emit any library filenames"
                .to_string(),
            status: None,
        });
    }

    Ok(filenames)
}

fn apply_jvm_cargo_package_selection(command: &mut Command, cargo_context: &JvmCargoContext) {
    command
        .arg("--manifest-path")
        .arg(&cargo_context.cargo_manifest_path);
    if let Some(package_selector) = cargo_context.package_selector.as_deref() {
        command.arg("-p").arg(package_selector);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        JniIncludeDirectories, JniLinkerArgs, bundled_jvm_shared_library_path,
        clang_cl_jni_linker_args, clang_native_static_library_flags, clang_style_jni_linker_args,
        existing_jvm_shared_library_path, extract_library_filenames, extract_link_search_paths,
        extract_native_static_libraries, link_search_path_flags, msvc_link_search_path_flags,
        msvc_native_static_library_flags, msvc_rustflag_linker_args, parse_native_static_libraries,
        resolve_jni_include_directories_with_overrides, resolve_jvm_native_link_input,
        select_windows_static_library_filename, target_specific_java_home_env_key,
        target_specific_java_include_env_key,
    };
    use crate::build::CargoBuildProfile;
    use crate::cli::CliError;
    use crate::pack::java::plan::{JvmCargoContext, JvmCrateOutputs};
    use crate::target::JavaHostTarget;

    fn temporary_directory(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    fn cargo_context(root: &Path, host_target: JavaHostTarget) -> JvmCargoContext {
        JvmCargoContext {
            host_target,
            rust_target_triple: "x86_64-unknown-linux-gnu".to_string(),
            glibc_version: None,
            release: false,
            build_profile: CargoBuildProfile::Debug,
            artifact_name: "demo".to_string(),
            cargo_manifest_path: root.join("Cargo.toml"),
            manifest_path: root.join("Cargo.toml"),
            package_selector: None,
            target_directory: root.join("target"),
            cargo_command_args: Vec::new(),
            toolchain_selector: None,
            crate_outputs: JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: false,
            },
            cargo_build_command: None,
        }
    }

    #[test]
    fn parses_native_static_library_flags_from_cargo_output() {
        let parsed = parse_native_static_libraries(
            "note: native-static-libs: -framework Security -lresolv -lc++",
        )
        .expect("expected static library flags");

        assert_eq!(parsed, vec!["-framework", "Security", "-lresolv", "-lc++"]);
    }

    #[test]
    fn parses_native_static_library_flags_from_ansi_colored_cargo_output() {
        let parsed =
            parse_native_static_libraries("note: native-static-libs: -lSystem -lc -lm\u{1b}[0m")
                .expect("expected static library flags");

        assert_eq!(parsed, vec!["-lSystem", "-lc", "-lm"]);
    }

    #[test]
    fn preserves_repeated_framework_prefixes_in_native_static_library_flags() {
        let parsed = parse_native_static_libraries(
            "note: native-static-libs: -framework Security -framework SystemConfiguration -lobjc",
        )
        .expect("expected static library flags");

        assert_eq!(
            parsed,
            vec![
                "-framework",
                "Security",
                "-framework",
                "SystemConfiguration",
                "-lobjc",
            ]
        );
    }

    #[test]
    fn extracts_last_native_static_library_line_from_combined_output() {
        let parsed = extract_native_static_libraries(
            "Compiling demo\nnote: native-static-libs: -lSystem\nFinished\nnote: native-static-libs: -framework CoreFoundation -lSystem\n",
        )
        .expect("expected static library flags");

        assert_eq!(parsed, vec!["-framework", "CoreFoundation", "-lSystem"]);
    }

    #[test]
    fn extracts_link_search_paths_from_build_script_messages() {
        let linked_paths = extract_link_search_paths(
            r#"{"reason":"compiler-artifact","package_id":"path+file:///tmp/demo#0.1.0"}
{"reason":"build-script-executed","package_id":"path+file:///tmp/dep#0.1.0","linked_paths":["native=/tmp/out","framework=/tmp/frameworks","native=/tmp/out"]}"#,
        );

        assert_eq!(
            linked_paths,
            vec![
                "native=/tmp/out".to_string(),
                "framework=/tmp/frameworks".to_string(),
            ]
        );
    }

    #[test]
    fn converts_link_search_paths_to_clang_flags() {
        let flags = link_search_path_flags(&[
            "native=/tmp/out".to_string(),
            "framework=/tmp/frameworks".to_string(),
            "dependency=/tmp/deps".to_string(),
            "/tmp/plain".to_string(),
            "native=/tmp/out".to_string(),
        ]);

        assert_eq!(
            flags,
            vec![
                "-L/tmp/out".to_string(),
                "-F/tmp/frameworks".to_string(),
                "-L/tmp/deps".to_string(),
                "-L/tmp/plain".to_string(),
            ]
        );
    }

    #[test]
    fn converts_link_search_paths_to_msvc_flags() {
        let flags = msvc_link_search_path_flags(&[
            "native=/tmp/out".to_string(),
            "dependency=/tmp/deps".to_string(),
            "framework=/tmp/frameworks".to_string(),
            "/tmp/plain".to_string(),
            "native=/tmp/out".to_string(),
        ]);

        assert_eq!(
            flags,
            vec![
                "/LIBPATH:/tmp/out".to_string(),
                "/LIBPATH:/tmp/deps".to_string(),
                "/LIBPATH:/tmp/plain".to_string(),
            ]
        );
    }

    #[test]
    fn converts_native_static_libraries_to_msvc_flags() {
        let flags = msvc_native_static_library_flags(&[
            "-l".to_string(),
            "bcrypt".to_string(),
            "-lws2_32".to_string(),
            "-l:custom.lib".to_string(),
            "userenv.lib".to_string(),
            "-framework".to_string(),
            "Security".to_string(),
        ]);

        assert_eq!(
            flags,
            vec![
                "bcrypt.lib".to_string(),
                "ws2_32.lib".to_string(),
                "custom.lib".to_string(),
                "userenv.lib".to_string(),
            ]
        );
    }

    #[test]
    fn strips_implicit_darwin_system_libraries_from_clang_flags() {
        let flags = clang_native_static_library_flags(
            JavaHostTarget::DarwinArm64,
            &[
                "-framework".to_string(),
                "Security".to_string(),
                "-lc".to_string(),
                "-l".to_string(),
                "m".to_string(),
                "-lSystem".to_string(),
                "-liconv".to_string(),
            ],
        );

        assert_eq!(
            flags,
            vec![
                "-framework".to_string(),
                "Security".to_string(),
                "-liconv".to_string(),
            ]
        );
    }

    #[test]
    fn preserves_linux_system_libraries_in_clang_flags() {
        let flags = clang_native_static_library_flags(
            JavaHostTarget::LinuxX86_64,
            &[
                "-ldl".to_string(),
                "-lpthread".to_string(),
                "-lm".to_string(),
            ],
        );

        assert_eq!(
            flags,
            vec![
                "-ldl".to_string(),
                "-lpthread".to_string(),
                "-lm".to_string(),
            ]
        );
    }

    #[test]
    fn converts_msvc_rustflag_linker_args() {
        let flags = msvc_rustflag_linker_args(&[
            "-L/tmp/native".to_string(),
            "-lws2_32".to_string(),
            "userenv.lib".to_string(),
            "/DEBUG".to_string(),
        ])
        .expect("msvc rustflag conversion");

        assert_eq!(
            flags,
            vec![
                "/LIBPATH:/tmp/native".to_string(),
                "ws2_32.lib".to_string(),
                "userenv.lib".to_string(),
                "/DEBUG".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_unsupported_msvc_rustflag_linker_args() {
        let error = msvc_rustflag_linker_args(&["-Wl,--as-needed".to_string()])
            .expect_err("unsupported flag should fail");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("-Wl,--as-needed")
        ));
    }

    #[test]
    fn builds_clang_cl_jni_linker_args_with_msvc_flags() {
        let include_directories = JniIncludeDirectories {
            shared: PathBuf::from("/tmp/jdk/include"),
            platform: PathBuf::from("/tmp/jdk/include/win32"),
        };

        let args = clang_cl_jni_linker_args(&JniLinkerArgs {
            host_target: JavaHostTarget::WindowsX86_64,
            output_lib: Path::new("/tmp/out/demo_jni.dll"),
            jni_glue: Path::new("/tmp/jni/jni_glue.c"),
            link_input: Path::new("/tmp/target/demo.lib"),
            jni_dir: Path::new("/tmp/jni"),
            jni_include_directories: &include_directories,
            rustflag_linker_args: &["-L/tmp/rustflag-native".to_string(), "-luser32".to_string()],
            native_link_search_paths: &["native=/tmp/native".to_string()],
            native_static_libraries: &["-lws2_32".to_string(), "userenv.lib".to_string()],
            rpath_flag: None,
        })
        .expect("msvc jni args");

        assert_eq!(
            args,
            vec![
                "/LD".to_string(),
                "/tmp/jni/jni_glue.c".to_string(),
                "/tmp/target/demo.lib".to_string(),
                "/I/tmp/jni".to_string(),
                "/I/tmp/jdk/include".to_string(),
                "/I/tmp/jdk/include/win32".to_string(),
                "/link".to_string(),
                "/OUT:/tmp/out/demo_jni.dll".to_string(),
                "/LIBPATH:/tmp/rustflag-native".to_string(),
                "user32.lib".to_string(),
                "/LIBPATH:/tmp/native".to_string(),
                "ws2_32.lib".to_string(),
                "userenv.lib".to_string(),
            ]
        );
    }

    #[test]
    fn strips_implicit_darwin_system_libraries_from_clang_jni_args() {
        let include_directories = JniIncludeDirectories {
            shared: PathBuf::from("/tmp/jdk/include"),
            platform: PathBuf::from("/tmp/jdk/include/darwin"),
        };

        let args = clang_style_jni_linker_args(&JniLinkerArgs {
            host_target: JavaHostTarget::DarwinArm64,
            output_lib: Path::new("/tmp/out/libdemo_jni.dylib"),
            jni_glue: Path::new("/tmp/jni/jni_glue.c"),
            link_input: Path::new("/tmp/target/libdemo.a"),
            jni_dir: Path::new("/tmp/jni"),
            jni_include_directories: &include_directories,
            rustflag_linker_args: &[],
            native_link_search_paths: &[],
            native_static_libraries: &[
                "-framework".to_string(),
                "Security".to_string(),
                "-lc".to_string(),
                "-lm".to_string(),
                "-lSystem".to_string(),
                "-liconv".to_string(),
            ],
            rpath_flag: Some("-Wl,-rpath,@loader_path"),
        });

        assert_eq!(
            args,
            vec![
                "-shared".to_string(),
                "-fPIC".to_string(),
                "-o".to_string(),
                "/tmp/out/libdemo_jni.dylib".to_string(),
                "-x".to_string(),
                "c".to_string(),
                "/tmp/jni/jni_glue.c".to_string(),
                "-x".to_string(),
                "none".to_string(),
                "/tmp/target/libdemo.a".to_string(),
                "-I/tmp/jni".to_string(),
                "-I/tmp/jdk/include".to_string(),
                "-I/tmp/jdk/include/darwin".to_string(),
                "-framework".to_string(),
                "Security".to_string(),
                "-liconv".to_string(),
                "-Wl,-rpath,@loader_path".to_string(),
            ]
        );
    }

    #[test]
    fn extracts_library_filenames_from_print_file_names_output() {
        let filenames = extract_library_filenames(
            "Compiling demo\nlibdemo.a\nlibdemo.dylib\nlibdemo.rlib\nFinished\n",
        );

        assert_eq!(
            filenames,
            vec![
                "libdemo.a".to_string(),
                "libdemo.dylib".to_string(),
                "libdemo.rlib".to_string(),
            ]
        );
    }

    #[test]
    fn selects_windows_static_library_filename_from_reported_outputs() {
        let filename = select_windows_static_library_filename(
            "demo",
            &[
                "demo.lib".to_string(),
                "demo.dll".to_string(),
                "demo.rlib".to_string(),
            ],
        )
        .expect("expected windows staticlib filename");

        assert_eq!(filename, "demo.lib");
    }

    #[test]
    fn selects_windows_gnu_static_library_filename_from_reported_outputs() {
        let filename = select_windows_static_library_filename(
            "demo",
            &[
                "libdemo.a".to_string(),
                "demo.dll".to_string(),
                "demo.rlib".to_string(),
            ],
        )
        .expect("expected windows gnu staticlib filename");

        assert_eq!(filename, "libdemo.a");
    }

    #[test]
    fn builds_target_specific_java_env_keys() {
        assert_eq!(
            target_specific_java_home_env_key("x86_64-unknown-linux-gnu"),
            "BOLTFFI_JAVA_HOME_X86_64_UNKNOWN_LINUX_GNU"
        );
        assert_eq!(
            target_specific_java_include_env_key("x86_64-unknown-linux-gnu"),
            "BOLTFFI_JAVA_INCLUDE_X86_64_UNKNOWN_LINUX_GNU"
        );
    }

    #[test]
    fn rejects_missing_cross_host_jni_headers_during_validation() {
        let temp_root = temporary_directory("boltffi-java-headers-test");
        let java_home = temp_root.join("linux-jdk");
        let shared_include = java_home.join("include");
        fs::create_dir_all(&shared_include).expect("create shared include dir");
        fs::write(shared_include.join("jni.h"), []).expect("write jni.h");

        let error = resolve_jni_include_directories_with_overrides(
            &cargo_context(&temp_root, JavaHostTarget::LinuxX86_64),
            Some(java_home),
            None,
            None,
        )
        .expect_err("expected missing target headers error");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("BOLTFFI_JAVA_INCLUDE_X86_64_UNKNOWN_LINUX_GNU")
                    && command.contains("BOLTFFI_JAVA_HOME_X86_64_UNKNOWN_LINUX_GNU")
        ));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn rejects_missing_jni_header_files_during_validation() {
        let temp_root = temporary_directory("boltffi-java-header-files-test");
        let shared_include = temp_root.join("include");
        let platform_include = shared_include.join("linux");
        fs::create_dir_all(&platform_include).expect("create include dirs");

        let error = resolve_jni_include_directories_with_overrides(
            &cargo_context(&temp_root, JavaHostTarget::LinuxX86_64),
            None,
            None,
            Some(platform_include),
        )
        .expect_err("expected missing header file error");

        assert!(matches!(error, CliError::FileNotFound(path) if path.ends_with("jni.h")));

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn accepts_target_include_override_without_java_home() {
        let temp_root = temporary_directory("boltffi-java-include-only-test");
        let shared_include = temp_root.join("include");
        let platform_include = shared_include.join("linux");
        fs::create_dir_all(&platform_include).expect("create platform include dir");
        fs::write(shared_include.join("jni.h"), []).expect("write jni.h");
        fs::write(platform_include.join("jni_md.h"), []).expect("write jni_md.h");

        let include_directories = resolve_jni_include_directories_with_overrides(
            &cargo_context(&temp_root, JavaHostTarget::LinuxX86_64),
            None,
            None,
            Some(platform_include.clone()),
        )
        .expect("include override should be sufficient");

        assert_eq!(include_directories.shared, shared_include);
        assert_eq!(include_directories.platform, platform_include);

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn prefers_target_include_override_over_java_home_include() {
        let temp_root = temporary_directory("boltffi-java-include-priority-test");
        let host_java_home = temp_root.join("host-jdk");
        let target_include_root = temp_root.join("target-jdk").join("include");
        let target_platform_include = target_include_root.join("linux");
        fs::create_dir_all(host_java_home.join("include").join("darwin"))
            .expect("create host include dir");
        fs::create_dir_all(&target_platform_include).expect("create target include dir");
        fs::write(host_java_home.join("include").join("jni.h"), []).expect("write host jni.h");
        fs::write(target_include_root.join("jni.h"), []).expect("write target jni.h");
        fs::write(target_platform_include.join("jni_md.h"), []).expect("write target jni_md.h");

        let include_directories = resolve_jni_include_directories_with_overrides(
            &cargo_context(&temp_root, JavaHostTarget::LinuxX86_64),
            Some(host_java_home),
            None,
            Some(target_platform_include.clone()),
        )
        .expect("target include override should take precedence");

        assert_eq!(include_directories.shared, target_include_root);
        assert_eq!(include_directories.platform, target_platform_include);

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn prefers_staticlib_for_jvm_linking_when_available() {
        let temp_root = temporary_directory("boltffi-jvm-link-test");
        let profile_dir = temp_root.join("release");
        fs::create_dir_all(&profile_dir).expect("create profile dir");

        let staticlib = profile_dir.join("libdemo.a");
        let cdylib = profile_dir.join("libdemo.dylib");
        fs::write(&staticlib, []).expect("write staticlib");
        fs::write(&cdylib, []).expect("write cdylib");

        let resolved = resolve_jvm_native_link_input(
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: true,
            },
            Some("libdemo.a"),
        )
        .expect("expected link input");

        assert_eq!(resolved.path(), staticlib.as_path());

        fs::remove_dir_all(&temp_root).expect("cleanup temp target dir");
    }

    #[test]
    fn skips_shared_library_compatibility_copy_when_jni_links_staticlib() {
        let temp_root = temporary_directory("boltffi-jvm-copy-test");
        let profile_dir = temp_root.join("release");
        fs::create_dir_all(&profile_dir).expect("create profile dir");

        let staticlib = profile_dir.join("libdemo.a");
        let cdylib = profile_dir.join("libdemo.dylib");
        fs::write(&staticlib, []).expect("write staticlib");
        fs::write(&cdylib, []).expect("write cdylib");

        let resolved = resolve_jvm_native_link_input(
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: true,
            },
            Some("libdemo.a"),
        )
        .expect("expected link input");
        let compatibility_shared_library = bundled_jvm_shared_library_path(
            &resolved,
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: true,
            },
        );

        assert_eq!(resolved.path(), staticlib.as_path());
        assert!(compatibility_shared_library.is_none());

        fs::remove_dir_all(&temp_root).expect("cleanup temp target dir");
    }

    #[test]
    fn keeps_shared_library_compatibility_copy_when_jni_links_cdylib() {
        let temp_root = temporary_directory("boltffi-jvm-copy-cdylib-test");
        let profile_dir = temp_root.join("release");
        fs::create_dir_all(&profile_dir).expect("create profile dir");

        let cdylib = profile_dir.join("libdemo.dylib");
        fs::write(&cdylib, []).expect("write cdylib");

        let resolved = resolve_jvm_native_link_input(
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: false,
                builds_cdylib: true,
            },
            None,
        )
        .expect("expected link input");
        let compatibility_shared_library = bundled_jvm_shared_library_path(
            &resolved,
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: false,
                builds_cdylib: true,
            },
        )
        .expect("expected shared library compatibility copy");

        assert_eq!(resolved.path(), cdylib.as_path());
        assert_eq!(compatibility_shared_library, cdylib);

        fs::remove_dir_all(&temp_root).expect("cleanup temp target dir");
    }

    #[test]
    fn ignores_stale_staticlib_when_current_crate_is_cdylib_only() {
        let temp_root = temporary_directory("boltffi-jvm-stale-static");
        let profile_dir = temp_root.join("release");
        fs::create_dir_all(&profile_dir).expect("create profile dir");

        let staticlib = profile_dir.join("libdemo.a");
        let cdylib = profile_dir.join("libdemo.dylib");
        fs::write(&staticlib, []).expect("write stale staticlib");
        fs::write(&cdylib, []).expect("write current cdylib");

        let resolved = resolve_jvm_native_link_input(
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: false,
                builds_cdylib: true,
            },
            None,
        )
        .expect("expected link input");

        assert_eq!(resolved.path(), cdylib.as_path());

        fs::remove_dir_all(&temp_root).expect("cleanup temp target dir");
    }

    #[test]
    fn ignores_stale_shared_library_when_current_crate_is_staticlib_only() {
        let temp_root = temporary_directory("boltffi-jvm-stale-cdylib");
        let profile_dir = temp_root.join("release");
        fs::create_dir_all(&profile_dir).expect("create profile dir");

        let cdylib = profile_dir.join("libdemo.dylib");
        fs::write(&cdylib, []).expect("write stale shared library");

        let compatibility_shared_library = existing_jvm_shared_library_path(
            &profile_dir,
            JavaHostTarget::DarwinArm64,
            "demo",
            JvmCrateOutputs {
                builds_staticlib: true,
                builds_cdylib: false,
            },
        );

        assert!(compatibility_shared_library.is_none());

        fs::remove_dir_all(&temp_root).expect("cleanup temp target dir");
    }
}
