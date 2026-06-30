use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::build::{CargoBuildProfile, OutputCallback, run_command_streaming};
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

#[derive(Debug, Clone)]
pub(crate) struct JvmNativePackageLayout {
    pub(crate) jni_dir: PathBuf,
    pub(crate) header_name: String,
    pub(crate) jni_library_name: String,
    pub(crate) native_output_root: PathBuf,
    pub(crate) flat_output_root: Option<PathBuf>,
    pub(crate) strip_symbols: bool,
    pub(crate) debug_symbols_enabled: bool,
}

impl JvmNativePackageLayout {
    pub(crate) fn java(config: &Config, artifact_name: &str) -> Self {
        let java_output = config.java_jvm_output();

        Self {
            jni_dir: java_output.join("jni"),
            header_name: artifact_name.to_string(),
            jni_library_name: artifact_name.to_string(),
            native_output_root: java_output.join("native"),
            flat_output_root: Some(java_output),
            strip_symbols: config.java_jvm_strip_symbols(),
            debug_symbols_enabled: config.java_jvm_debug_symbols_enabled(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopJniStripMode {
    Disabled,
    Automatic,
    Explicit,
}

#[cfg(test)]
fn desktop_jni_strip_mode(
    config: &Config,
    build_profile: &CargoBuildProfile,
) -> DesktopJniStripMode {
    desktop_jni_strip_mode_for(config.java_jvm_strip_symbols(), build_profile)
}

fn desktop_jni_strip_mode_for(
    configured_strip_symbols: bool,
    build_profile: &CargoBuildProfile,
) -> DesktopJniStripMode {
    if matches!(build_profile, CargoBuildProfile::Release) {
        DesktopJniStripMode::Automatic
    } else if matches!(build_profile, CargoBuildProfile::Named(_)) && configured_strip_symbols {
        DesktopJniStripMode::Explicit
    } else {
        DesktopJniStripMode::Disabled
    }
}

#[cfg(test)]
fn validate_desktop_jni_symbol_stripping(
    config: &Config,
    host_target: JavaHostTarget,
) -> Result<()> {
    validate_desktop_jni_symbol_stripping_for(config.java_jvm_strip_symbols(), host_target)
}

pub(crate) fn validate_desktop_jni_symbol_stripping_for(
    configured_strip_symbols: bool,
    host_target: JavaHostTarget,
) -> Result<()> {
    if configured_strip_symbols && matches!(host_target, JavaHostTarget::WindowsX86_64) {
        return Err(CliError::CommandFailed {
            command: "targets.java.jvm.strip_symbols is not supported for windows-x86_64 yet"
                .to_string(),
            status: None,
        });
    }

    Ok(())
}

fn strip_packaged_desktop_shared_library(
    strip_mode: DesktopJniStripMode,
    host_target: JavaHostTarget,
    jni_compiler_program: &Path,
    rust_target_triple: &str,
    library_path: &Path,
) -> Result<()> {
    let Some(mut command) = packaged_desktop_shared_library_strip_command(
        strip_mode,
        host_target,
        jni_compiler_program,
        rust_target_triple,
    )?
    else {
        return Ok(());
    };
    command.arg(library_path);

    let status = command.status().map_err(|source| CliError::CommandFailed {
        command: format!(
            "desktop strip tool failed to launch for '{}': {}",
            library_path.display(),
            source
        ),
        status: None,
    })?;

    if !status.success() {
        return Err(CliError::CommandFailed {
            command: format!(
                "desktop strip tool failed for '{}' on '{}'",
                library_path.display(),
                host_target.canonical_name()
            ),
            status: status.code(),
        });
    }

    Ok(())
}

fn packaged_desktop_shared_library_strip_command(
    strip_mode: DesktopJniStripMode,
    host_target: JavaHostTarget,
    jni_compiler_program: &Path,
    rust_target_triple: &str,
) -> Result<Option<Command>> {
    match host_target {
        JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64 => {
            let mut command = Command::new("xcrun");
            command.arg("strip").arg("-x");
            Ok(Some(command))
        }
        JavaHostTarget::LinuxX86_64 | JavaHostTarget::LinuxAarch64 => {
            let Some(strip_program) =
                resolve_linux_strip_program(strip_mode, jni_compiler_program, rust_target_triple)?
            else {
                return Ok(None);
            };
            let mut command = Command::new(strip_program);
            command.arg("--strip-all");
            Ok(Some(command))
        }
        JavaHostTarget::Current | JavaHostTarget::WindowsX86_64 => Ok(None),
    }
}

fn resolve_linux_strip_program(
    strip_mode: DesktopJniStripMode,
    jni_compiler_program: &Path,
    rust_target_triple: &str,
) -> Result<Option<PathBuf>> {
    for candidate in linux_strip_program_candidates(jni_compiler_program, rust_target_triple) {
        if let Ok(resolved) = which::which(&candidate) {
            return Ok(Some(resolved));
        }
    }

    let message = format!(
        "no supported Linux strip tool found to strip packaged JVM shared libraries (compiler: '{}')",
        jni_compiler_program.display()
    );

    handle_missing_linux_strip_program(strip_mode, &message)
}

fn handle_missing_linux_strip_program(
    strip_mode: DesktopJniStripMode,
    message: &str,
) -> Result<Option<PathBuf>> {
    match strip_mode {
        DesktopJniStripMode::Explicit => Err(CliError::CommandFailed {
            command: message.to_string(),
            status: None,
        }),
        DesktopJniStripMode::Automatic => {
            print_cargo_line(&format!("warning: {message}; skipping symbol stripping"));
            Ok(None)
        }
        DesktopJniStripMode::Disabled => Ok(None),
    }
}

fn linux_strip_program_candidates(
    jni_compiler_program: &Path,
    rust_target_triple: &str,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(directory) = jni_compiler_program.parent() {
        if let Some(prefix) = target_prefixed_binutils_prefix(jni_compiler_program) {
            push_unique_path(&mut candidates, directory.join(format!("{prefix}-strip")));
        }
        if let Some(version_suffix) = compiler_tool_version_suffix(jni_compiler_program) {
            push_unique_path(
                &mut candidates,
                directory.join(format!("llvm-strip-{version_suffix}")),
            );
        }
        push_unique_path(&mut candidates, directory.join("llvm-strip"));
    }

    for candidate in target_prefixed_strip_tool_candidates(rust_target_triple) {
        push_unique_path(&mut candidates, PathBuf::from(candidate));
    }
    if let Some(version_suffix) = compiler_tool_version_suffix(jni_compiler_program) {
        push_unique_path(
            &mut candidates,
            PathBuf::from(format!("llvm-strip-{version_suffix}")),
        );
    }
    push_unique_path(&mut candidates, PathBuf::from("llvm-strip"));

    if cfg!(target_os = "linux") {
        push_unique_path(&mut candidates, PathBuf::from("strip"));
    }

    candidates
}

fn target_prefixed_binutils_prefix(jni_compiler_program: &Path) -> Option<String> {
    let name = jni_compiler_program.file_name()?.to_str()?;
    let name = name.strip_suffix(".exe").unwrap_or(name);
    let mut parts = name.split('-').collect::<Vec<_>>();

    if parts
        .last()
        .is_some_and(|part| is_numeric_tool_suffix(part))
    {
        parts.pop();
    }

    let compiler = parts.pop()?;
    if !matches!(compiler, "clang" | "gcc" | "cc") || parts.is_empty() {
        return None;
    }

    Some(parts.join("-"))
}

fn compiler_tool_version_suffix(jni_compiler_program: &Path) -> Option<String> {
    let name = jni_compiler_program.file_name()?.to_str()?;
    let name = name.strip_suffix(".exe").unwrap_or(name);
    let parts = name.split('-').collect::<Vec<_>>();
    let version_suffix = parts.last()?;

    is_numeric_tool_suffix(version_suffix).then(|| (*version_suffix).to_string())
}

fn target_prefixed_strip_tool_candidates(rust_target_triple: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    if let Some(vendorless_triple) = vendorless_linux_target_triple(rust_target_triple) {
        candidates.push(format!("{vendorless_triple}-strip"));
    }

    candidates.push(format!("{rust_target_triple}-strip"));
    candidates.dedup();
    candidates
}

fn vendorless_linux_target_triple(rust_target_triple: &str) -> Option<String> {
    let parts = rust_target_triple.split('-').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }

    let vendorless = std::iter::once(parts[0])
        .chain(parts[2..].iter().copied())
        .collect::<Vec<_>>()
        .join("-");

    (vendorless != rust_target_triple && vendorless.contains("linux")).then_some(vendorless)
}

fn is_numeric_tool_suffix(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
}

fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.contains(&candidate) {
        paths.push(candidate);
    }
}

pub(crate) struct JniLinkerArgs<'a> {
    pub(crate) host_target: JavaHostTarget,
    pub(crate) release: bool,
    pub(crate) output_lib: &'a Path,
    pub(crate) jni_glue: &'a Path,
    pub(crate) link_input: &'a Path,
    pub(crate) jni_dir: &'a Path,
    pub(crate) jni_include_directories: &'a JniIncludeDirectories,
    pub(crate) rustflag_linker_args: &'a [String],
    pub(crate) native_link_search_paths: &'a [String],
    pub(crate) native_static_libraries: &'a [String],
    pub(crate) rpath_flag: Option<&'a str>,
    pub(crate) emit_debug_info: bool,
}

pub(crate) struct NativeLinkMetadata {
    pub(crate) native_static_libraries: Vec<String>,
    pub(crate) native_link_search_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct JvmPackagedNativeOutput {
    pub(crate) host_target: JavaHostTarget,
    pub(crate) has_shared_library_copy: bool,
    pub(crate) jni_library_path: PathBuf,
    pub(crate) shared_library_path: Option<PathBuf>,
    pub(crate) debug_info_sidecars: Vec<PathBuf>,
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
    let layout =
        JvmNativePackageLayout::java(config, &packaging_target.cargo_context.artifact_name);
    compile_jni_library_with_layout(packaging_target, build_artifacts, &layout, step)
}

pub(crate) fn compile_jni_library_with_layout(
    packaging_target: &JvmPackagingTarget,
    build_artifacts: &JvmBuildArtifacts,
    layout: &JvmNativePackageLayout,
    step: &Step,
) -> Result<JvmPackagedNativeOutput> {
    let cargo_context = &packaging_target.cargo_context;
    let host_target = cargo_context.host_target;
    validate_desktop_jni_symbol_stripping_for(layout.strip_symbols, host_target)?;
    let strip_mode = desktop_jni_strip_mode_for(layout.strip_symbols, &cargo_context.build_profile);
    let strip_symbols = !matches!(strip_mode, DesktopJniStripMode::Disabled);
    let jni_dir = &layout.jni_dir;
    let jni_glue = jni_dir.join("jni_glue.c");
    let header = jni_dir.join(format!("{}.h", layout.header_name));

    if !jni_glue.exists() {
        return Err(CliError::FileNotFound(jni_glue));
    }
    if !header.exists() {
        return Err(CliError::FileNotFound(header));
    }

    let artifact_name = &cargo_context.artifact_name;
    let jni_library_name = &layout.jni_library_name;
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

    let host_native_output = layout.native_output_root.join(host_target.canonical_name());
    std::fs::create_dir_all(&host_native_output).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: host_native_output.clone(),
            source,
        }
    })?;

    let output_lib = host_native_output.join(host_target.jni_library_filename(jni_library_name));
    let jni_include_directories = resolve_jni_include_directories(cargo_context)?;
    let has_shared_library_copy = compatibility_shared_library.is_some();
    let mut debug_info_sidecars = Vec::new();

    let mut command = packaging_target.toolchain.linker_command();
    let jni_linker_args = if packaging_target.toolchain.uses_msvc_compiler() {
        clang_cl_jni_linker_args(&JniLinkerArgs {
            host_target,
            release: strip_symbols,
            output_lib: &output_lib,
            jni_glue: &jni_glue,
            link_input: link_input.path(),
            jni_dir,
            jni_include_directories: &jni_include_directories,
            rustflag_linker_args: packaging_target.toolchain.jni_rustflag_linker_args(),
            native_link_search_paths: &build_artifacts.native_link_search_paths,
            native_static_libraries: &build_artifacts.native_static_libraries,
            rpath_flag: None,
            emit_debug_info: layout.debug_symbols_enabled,
        })?
    } else {
        clang_style_jni_linker_args(&JniLinkerArgs {
            host_target,
            release: strip_symbols,
            output_lib: &output_lib,
            jni_glue: &jni_glue,
            link_input: link_input.path(),
            jni_dir,
            jni_include_directories: &jni_include_directories,
            rustflag_linker_args: packaging_target.toolchain.jni_rustflag_linker_args(),
            native_link_search_paths: &build_artifacts.native_link_search_paths,
            native_static_libraries: &build_artifacts.native_static_libraries,
            rpath_flag: host_target.rpath_flag(),
            emit_debug_info: layout.debug_symbols_enabled,
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
    if current_host == Some(host_target)
        && let Some(flat_output_root) = &layout.flat_output_root
    {
        let compatibility_jni_copy =
            flat_output_root.join(host_target.jni_library_filename(jni_library_name));
        std::fs::copy(&output_lib, &compatibility_jni_copy).map_err(|source| {
            CliError::CopyFailed {
                from: output_lib.clone(),
                to: compatibility_jni_copy,
                source,
            }
        })?;
    }
    debug_info_sidecars.extend(existing_windows_pdb_sidecars(&output_lib));
    if should_generate_apple_dsym_sidecars(layout.debug_symbols_enabled, &output_lib, host_target) {
        debug_info_sidecars.extend(generate_apple_dsym_sidecars(
            &output_lib,
            host_target,
            step,
        )?);
    }

    let mut shared_library_path = None;
    if let Some(shared_library) = compatibility_shared_library.as_deref() {
        let shared_library_name = shared_library
            .file_name()
            .expect("shared library path should have a file name");
        let structured_copy = host_native_output.join(shared_library_name);
        std::fs::copy(shared_library, &structured_copy).map_err(|source| CliError::CopyFailed {
            from: shared_library.to_path_buf(),
            to: structured_copy.clone(),
            source,
        })?;

        if strip_symbols {
            strip_packaged_desktop_shared_library(
                strip_mode,
                host_target,
                packaging_target.toolchain.jni_compiler_program(),
                packaging_target.toolchain.rust_target_triple(),
                &structured_copy,
            )?;
        }

        if current_host == Some(host_target)
            && let Some(flat_output_root) = &layout.flat_output_root
        {
            let flat_copy = flat_output_root.join(shared_library_name);
            std::fs::copy(&structured_copy, &flat_copy).map_err(|source| CliError::CopyFailed {
                from: structured_copy.clone(),
                to: flat_copy,
                source,
            })?;
        }

        shared_library_path = Some(structured_copy);
        debug_info_sidecars.extend(existing_windows_pdb_sidecars(shared_library));
        let shared_library_path = shared_library_path
            .as_deref()
            .expect("shared library copy should be set");
        if should_generate_apple_dsym_sidecars(
            layout.debug_symbols_enabled,
            shared_library_path,
            host_target,
        ) {
            debug_info_sidecars.extend(generate_apple_dsym_sidecars(
                shared_library_path,
                host_target,
                step,
            )?);
        }
    }

    Ok(JvmPackagedNativeOutput {
        host_target,
        has_shared_library_copy,
        jni_library_path: output_lib,
        shared_library_path,
        debug_info_sidecars,
    })
}

fn existing_windows_pdb_sidecars(path: &Path) -> Vec<PathBuf> {
    let pdb_path = path.with_extension("pdb");
    pdb_path.exists().then_some(pdb_path).into_iter().collect()
}

fn should_generate_apple_dsym_sidecars(
    debug_symbols_enabled: bool,
    path: &Path,
    host_target: JavaHostTarget,
) -> bool {
    debug_symbols_enabled
        && cfg!(target_os = "macos")
        && matches!(
            host_target,
            JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64
        )
        && path.extension().and_then(|extension| extension.to_str()) == Some("dylib")
}

fn generate_apple_dsym_sidecars(
    path: &Path,
    host_target: JavaHostTarget,
    step: &Step,
) -> Result<Vec<PathBuf>> {
    if !should_generate_apple_dsym_sidecars(true, path, host_target) {
        return Ok(Vec::new());
    }

    let dsym_path = apple_dsym_bundle_path(path);
    if dsym_path.exists() {
        std::fs::remove_dir_all(&dsym_path).map_err(|source| CliError::WriteFailed {
            path: dsym_path.clone(),
            source,
        })?;
    }

    let mut command = Command::new("dsymutil");
    command.arg(path).arg("-o").arg(&dsym_path);
    if step.is_verbose() {
        print_verbose_detail(&format!(
            "Generating dSYM sidecar: {}",
            format_command_for_log(&command)
        ));
    }

    let status = command.status().map_err(|source| CliError::CommandFailed {
        command: format!("dsymutil: {source}"),
        status: None,
    })?;
    if !status.success() {
        return Err(CliError::CommandFailed {
            command: format!("dsymutil failed for '{}'", path.display()),
            status: status.code(),
        });
    }

    Ok(dsym_path
        .exists()
        .then_some(dsym_path)
        .into_iter()
        .collect())
}

fn apple_dsym_bundle_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .expect("dSYM source path should have a file name")
        .to_string_lossy();
    path.parent()
        .expect("dSYM source path should have a parent")
        .join(format!("{file_name}.dSYM"))
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
    let mut command = Command::new("cargo");
    command.current_dir(crate_directory);

    if let Some(toolchain_selector) = cargo_context.toolchain_selector.as_deref() {
        command.arg(toolchain_selector);
    }

    command
        .arg("build")
        .arg("--target")
        .arg(&cargo_context.rust_target_triple);
    apply_jvm_cargo_package_selection(&mut command, cargo_context);

    if release {
        command.arg("--release");
    }

    command.args(&cargo_context.cargo_command_args);
    packaging_target
        .toolchain
        .configure_cargo_build(&mut command);

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
    let mut resolved_args = vec!["-shared".to_string(), "-fPIC".to_string()];
    if args.emit_debug_info {
        resolved_args.push("-g".to_string());
    }
    resolved_args.extend([
        "-o".to_string(),
        args.output_lib.display().to_string(),
        args.jni_glue.display().to_string(),
        args.link_input.display().to_string(),
        format!("-I{}", args.jni_dir.display()),
        format!("-I{}", args.jni_include_directories.shared.display()),
        format!("-I{}", args.jni_include_directories.platform.display()),
    ]);
    resolved_args.extend(args.rustflag_linker_args.iter().cloned());
    resolved_args.extend(link_search_path_flags(args.native_link_search_paths));
    resolved_args.extend(clang_native_static_library_flags(
        args.host_target,
        args.native_static_libraries,
    ));
    resolved_args.extend(clang_release_optimization_flags(
        args.host_target,
        args.release,
    ));
    if let Some(rpath_flag) = args.rpath_flag {
        resolved_args.push(rpath_flag.to_string());
    }
    resolved_args
}

pub(crate) fn clang_release_optimization_flags(
    host_target: JavaHostTarget,
    release: bool,
) -> Vec<String> {
    if !release {
        return Vec::new();
    }

    match host_target {
        JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64 => vec!["-Wl,-x".to_string()],
        JavaHostTarget::LinuxX86_64 | JavaHostTarget::LinuxAarch64 => {
            vec!["-Wl,--strip-all".to_string()]
        }
        JavaHostTarget::Current | JavaHostTarget::WindowsX86_64 => Vec::new(),
    }
}

pub(crate) fn clang_cl_jni_linker_args(args: &JniLinkerArgs<'_>) -> Result<Vec<String>> {
    let mut resolved_args = vec!["/LD".to_string()];
    if args.emit_debug_info {
        resolved_args.push("/Z7".to_string());
    }
    resolved_args.extend([
        args.jni_glue.display().to_string(),
        args.link_input.display().to_string(),
        format!("/I{}", args.jni_dir.display()),
        format!("/I{}", args.jni_include_directories.shared.display()),
        format!("/I{}", args.jni_include_directories.platform.display()),
        "/link".to_string(),
        format!("/OUT:{}", args.output_lib.display()),
    ]);
    if args.emit_debug_info {
        resolved_args.push("/DEBUG".to_string());
    }
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
    let resolved_java_home = target_java_home_override
        .as_deref()
        .and_then(|java_home| resolve_java_home_for_target(java_home, cargo_context.host_target))
        .or_else(|| {
            default_java_home.as_deref().and_then(|java_home| {
                resolve_java_home_for_target(java_home, cargo_context.host_target)
            })
        })
        .or(target_java_home_override.clone())
        .or(default_java_home.clone());

    let platform_include = target_include_override.clone().unwrap_or_else(|| {
        resolved_java_home
            .clone()
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
        .or_else(|| resolved_java_home.map(|java_home| java_home.join("include")))
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

fn resolve_java_home_for_target(java_home: &Path, host_target: JavaHostTarget) -> Option<PathBuf> {
    candidate_java_home_roots(java_home).find(|candidate| {
        let include_root = candidate.join("include");
        include_root.join("jni.h").exists()
            && include_root
                .join(host_target.jni_platform())
                .join("jni_md.h")
                .exists()
    })
}

fn candidate_java_home_roots(java_home: &Path) -> impl Iterator<Item = PathBuf> {
    let java_home = java_home.to_path_buf();
    let macos_bundle_home = java_home.join("Contents").join("Home");
    let homebrew_bundle_home = java_home
        .join("libexec")
        .join("openjdk.jdk")
        .join("Contents")
        .join("Home");

    [
        java_home.clone(),
        macos_bundle_home.clone(),
        homebrew_bundle_home,
        java_home.join("Home"),
        macos_bundle_home.join("Home"),
    ]
    .into_iter()
    .scan(Vec::<PathBuf>::new(), |seen, candidate| {
        if seen.iter().any(|existing| existing == &candidate) {
            Some(None)
        } else {
            seen.push(candidate.clone());
            Some(Some(candidate))
        }
    })
    .flatten()
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

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::{
        DesktopJniStripMode, JniIncludeDirectories, JniLinkerArgs, apple_dsym_bundle_path,
        bundled_jvm_shared_library_path, clang_cl_jni_linker_args,
        clang_native_static_library_flags, clang_release_optimization_flags,
        clang_style_jni_linker_args, compiler_tool_version_suffix, desktop_jni_strip_mode,
        existing_jvm_shared_library_path, extract_library_filenames, extract_link_search_paths,
        extract_native_static_libraries, handle_missing_linux_strip_program,
        link_search_path_flags, linux_strip_program_candidates, msvc_link_search_path_flags,
        msvc_native_static_library_flags, msvc_rustflag_linker_args, parse_native_static_libraries,
        resolve_jni_include_directories_with_overrides, resolve_jvm_native_link_input,
        resolve_linux_strip_program, select_windows_static_library_filename,
        should_generate_apple_dsym_sidecars, target_prefixed_binutils_prefix,
        target_prefixed_strip_tool_candidates, target_specific_java_home_env_key,
        target_specific_java_include_env_key, validate_desktop_jni_symbol_stripping,
        vendorless_linux_target_triple,
    };
    use crate::build::CargoBuildProfile;
    use crate::cli::CliError;
    use crate::config::{CargoConfig, Config, PackageConfig, TargetsConfig};
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
            release: false,
            build_profile: CargoBuildProfile::Debug,
            package_name: "demo".to_string(),
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
        }
    }

    fn config(strip_symbols: bool) -> Config {
        let mut config = Config {
            experimental: Vec::new(),
            cargo: CargoConfig::default(),
            package: PackageConfig {
                name: "demo".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig::default(),
        };
        config.targets.java.jvm.strip_symbols = strip_symbols;
        config
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
    fn apple_dsym_bundle_path_appends_bundle_suffix() {
        let path = PathBuf::from("/tmp/native/libdemo.dylib");

        assert_eq!(
            apple_dsym_bundle_path(&path),
            PathBuf::from("/tmp/native/libdemo.dylib.dSYM")
        );
    }

    #[test]
    fn apple_dsym_generation_requires_debug_symbols_opt_in() {
        let path = PathBuf::from("/tmp/native/libdemo.dylib");

        assert!(!should_generate_apple_dsym_sidecars(
            false,
            &path,
            JavaHostTarget::DarwinArm64
        ));
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
            release: true,
            output_lib: Path::new("/tmp/out/demo_jni.dll"),
            jni_glue: Path::new("/tmp/jni/jni_glue.c"),
            link_input: Path::new("/tmp/target/demo.lib"),
            jni_dir: Path::new("/tmp/jni"),
            jni_include_directories: &include_directories,
            rustflag_linker_args: &["-L/tmp/rustflag-native".to_string(), "-luser32".to_string()],
            native_link_search_paths: &["native=/tmp/native".to_string()],
            native_static_libraries: &["-lws2_32".to_string(), "userenv.lib".to_string()],
            rpath_flag: None,
            emit_debug_info: false,
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
            release: false,
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
            emit_debug_info: false,
        });

        assert_eq!(
            args,
            vec![
                "-shared".to_string(),
                "-fPIC".to_string(),
                "-o".to_string(),
                "/tmp/out/libdemo_jni.dylib".to_string(),
                "/tmp/jni/jni_glue.c".to_string(),
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
    fn clang_style_jni_linker_args_include_debug_info_when_requested() {
        let include_directories = JniIncludeDirectories {
            shared: PathBuf::from("/tmp/jdk/include"),
            platform: PathBuf::from("/tmp/jdk/include/linux"),
        };

        let args = clang_style_jni_linker_args(&JniLinkerArgs {
            host_target: JavaHostTarget::LinuxX86_64,
            release: false,
            output_lib: Path::new("/tmp/out/libdemo_jni.so"),
            jni_glue: Path::new("/tmp/jni/jni_glue.c"),
            link_input: Path::new("/tmp/target/libdemo.a"),
            jni_dir: Path::new("/tmp/jni"),
            jni_include_directories: &include_directories,
            rustflag_linker_args: &[],
            native_link_search_paths: &[],
            native_static_libraries: &[],
            rpath_flag: None,
            emit_debug_info: true,
        });

        assert!(args.contains(&"-g".to_string()));
    }

    #[test]
    fn builds_clang_cl_jni_linker_args_with_debug_info_flags() {
        let include_directories = JniIncludeDirectories {
            shared: PathBuf::from("/tmp/jdk/include"),
            platform: PathBuf::from("/tmp/jdk/include/win32"),
        };

        let args = clang_cl_jni_linker_args(&JniLinkerArgs {
            host_target: JavaHostTarget::WindowsX86_64,
            release: false,
            output_lib: Path::new("/tmp/out/demo_jni.dll"),
            jni_glue: Path::new("/tmp/jni/jni_glue.c"),
            link_input: Path::new("/tmp/target/demo.lib"),
            jni_dir: Path::new("/tmp/jni"),
            jni_include_directories: &include_directories,
            rustflag_linker_args: &[],
            native_link_search_paths: &[],
            native_static_libraries: &[],
            rpath_flag: None,
            emit_debug_info: true,
        })
        .expect("msvc jni args with debug info");

        assert!(args.contains(&"/Z7".to_string()));
        assert!(args.contains(&"/DEBUG".to_string()));
    }

    #[test]
    fn adds_darwin_release_jni_strip_flags() {
        let flags = clang_release_optimization_flags(JavaHostTarget::DarwinArm64, true);

        assert_eq!(flags, vec!["-Wl,-x".to_string()]);
    }

    #[test]
    fn adds_linux_release_jni_strip_flags() {
        let flags = clang_release_optimization_flags(JavaHostTarget::LinuxX86_64, true);

        assert_eq!(flags, vec!["-Wl,--strip-all".to_string()]);
    }

    #[test]
    fn extracts_target_prefixed_strip_tool_prefixes() {
        assert_eq!(
            target_prefixed_binutils_prefix(Path::new("/tmp/x86_64-linux-gnu-clang")),
            Some("x86_64-linux-gnu".to_string())
        );
        assert_eq!(
            target_prefixed_binutils_prefix(Path::new("/tmp/aarch64-linux-gnu-gcc")),
            Some("aarch64-linux-gnu".to_string())
        );
        assert_eq!(
            target_prefixed_binutils_prefix(Path::new("/tmp/x86_64-linux-gnu-clang-18")),
            Some("x86_64-linux-gnu".to_string())
        );
        assert_eq!(
            target_prefixed_binutils_prefix(Path::new("/tmp/aarch64-linux-gnu-gcc-14.2")),
            Some("aarch64-linux-gnu".to_string())
        );
        assert_eq!(
            target_prefixed_binutils_prefix(Path::new("/tmp/clang")),
            None
        );
    }

    #[test]
    fn extracts_compiler_tool_version_suffixes() {
        assert_eq!(
            compiler_tool_version_suffix(Path::new("/tmp/clang-18")),
            Some("18".to_string())
        );
        assert_eq!(
            compiler_tool_version_suffix(Path::new("/tmp/x86_64-linux-gnu-clang-18")),
            Some("18".to_string())
        );
        assert_eq!(
            compiler_tool_version_suffix(Path::new("/tmp/aarch64-linux-gnu-gcc-14.2")),
            Some("14.2".to_string())
        );
        assert_eq!(compiler_tool_version_suffix(Path::new("/tmp/clang")), None);
    }

    #[test]
    fn linux_strip_program_candidates_include_sibling_tools() {
        let candidates = linux_strip_program_candidates(
            Path::new("/tmp/toolchain/bin/x86_64-linux-gnu-clang"),
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(
            candidates[0],
            PathBuf::from("/tmp/toolchain/bin/x86_64-linux-gnu-strip")
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("/tmp/toolchain/bin/llvm-strip")
        );
        assert_eq!(candidates[2], PathBuf::from("x86_64-linux-gnu-strip"));
    }

    #[test]
    fn linux_strip_program_candidates_include_versioned_llvm_strip_tools() {
        let candidates = linux_strip_program_candidates(
            Path::new("/tmp/toolchain/bin/clang-18"),
            "x86_64-unknown-linux-gnu",
        );

        assert_eq!(
            candidates[0],
            PathBuf::from("/tmp/toolchain/bin/llvm-strip-18")
        );
        assert_eq!(
            candidates[1],
            PathBuf::from("/tmp/toolchain/bin/llvm-strip")
        );
        assert_eq!(candidates[2], PathBuf::from("x86_64-linux-gnu-strip"));
        assert_eq!(candidates[4], PathBuf::from("llvm-strip-18"));
    }

    #[test]
    fn resolves_linux_strip_program_from_sibling_target_prefixed_tool() {
        let temp_root = temporary_directory("boltffi-linux-strip-test");
        let bin_dir = temp_root.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let compiler = bin_dir.join("x86_64-linux-gnu-clang");
        let strip_tool = bin_dir.join("x86_64-linux-gnu-strip");
        fs::write(&compiler, []).expect("write fake compiler");
        fs::write(&strip_tool, []).expect("write fake strip tool");
        #[cfg(unix)]
        {
            fs::set_permissions(&strip_tool, fs::Permissions::from_mode(0o755))
                .expect("mark fake strip executable");
        }

        let resolved = resolve_linux_strip_program(
            DesktopJniStripMode::Explicit,
            &compiler,
            "x86_64-unknown-linux-gnu",
        )
        .expect("resolve sibling strip tool")
        .expect("strip tool should be present");

        assert_eq!(resolved, strip_tool);

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn resolves_linux_strip_program_from_sibling_versioned_llvm_strip_tool() {
        let temp_root = temporary_directory("boltffi-linux-llvm-strip-test");
        let bin_dir = temp_root.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let compiler = bin_dir.join("clang-18");
        let strip_tool = bin_dir.join("llvm-strip-18");
        fs::write(&compiler, []).expect("write fake compiler");
        fs::write(&strip_tool, []).expect("write fake strip tool");
        #[cfg(unix)]
        {
            fs::set_permissions(&strip_tool, fs::Permissions::from_mode(0o755))
                .expect("mark fake strip executable");
        }

        let resolved = resolve_linux_strip_program(
            DesktopJniStripMode::Explicit,
            &compiler,
            "x86_64-unknown-linux-gnu",
        )
        .expect("resolve sibling versioned llvm-strip tool")
        .expect("strip tool should be present");

        assert_eq!(resolved, strip_tool);

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn skips_missing_linux_strip_program_for_automatic_release_stripping() {
        let resolved = handle_missing_linux_strip_program(
            DesktopJniStripMode::Automatic,
            "no supported Linux strip tool found",
        )
        .expect("automatic stripping should not fail without a strip tool");

        assert!(resolved.is_none());
    }

    #[test]
    fn rejects_missing_linux_strip_program_for_explicit_stripping() {
        let error = handle_missing_linux_strip_program(
            DesktopJniStripMode::Explicit,
            "no supported Linux strip tool found",
        )
        .expect_err("explicit stripping should fail without a strip tool");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("no supported Linux strip tool found")
        ));
    }

    #[test]
    fn derives_target_prefixed_strip_tool_candidates_from_linux_target_triple() {
        assert_eq!(
            target_prefixed_strip_tool_candidates("x86_64-unknown-linux-gnu"),
            vec![
                "x86_64-linux-gnu-strip".to_string(),
                "x86_64-unknown-linux-gnu-strip".to_string(),
            ]
        );
        assert_eq!(
            target_prefixed_strip_tool_candidates("aarch64-unknown-linux-gnu"),
            vec![
                "aarch64-linux-gnu-strip".to_string(),
                "aarch64-unknown-linux-gnu-strip".to_string(),
            ]
        );
        assert_eq!(
            target_prefixed_strip_tool_candidates("x86_64-alpine-linux-musl"),
            vec![
                "x86_64-linux-musl-strip".to_string(),
                "x86_64-alpine-linux-musl-strip".to_string(),
            ]
        );
    }

    #[test]
    fn derives_vendorless_linux_target_triples() {
        assert_eq!(
            vendorless_linux_target_triple("x86_64-unknown-linux-gnu"),
            Some("x86_64-linux-gnu".to_string())
        );
        assert_eq!(
            vendorless_linux_target_triple("aarch64-alpine-linux-musl"),
            Some("aarch64-linux-musl".to_string())
        );
        assert_eq!(vendorless_linux_target_triple("x86_64-linux-musl"), None);
    }

    #[test]
    fn skips_release_jni_optimization_flags_for_debug_builds() {
        let flags = clang_release_optimization_flags(JavaHostTarget::DarwinArm64, false);

        assert!(flags.is_empty());
    }

    #[test]
    fn strips_desktop_jni_symbols_for_release_and_opted_in_named_profiles() {
        let default_config = config(false);
        let opted_in_config = config(true);

        assert!(!matches!(
            desktop_jni_strip_mode(&default_config, &CargoBuildProfile::Release),
            DesktopJniStripMode::Disabled
        ));
        assert!(matches!(
            desktop_jni_strip_mode(&default_config, &CargoBuildProfile::Debug),
            DesktopJniStripMode::Disabled
        ));
        assert!(matches!(
            desktop_jni_strip_mode(
                &default_config,
                &CargoBuildProfile::Named("asan".to_string())
            ),
            DesktopJniStripMode::Disabled
        ));
        assert!(!matches!(
            desktop_jni_strip_mode(
                &opted_in_config,
                &CargoBuildProfile::Named("dist".to_string())
            ),
            DesktopJniStripMode::Disabled
        ));
    }

    #[test]
    fn distinguishes_automatic_and_explicit_desktop_jni_strip_modes() {
        assert_eq!(
            desktop_jni_strip_mode(&config(false), &CargoBuildProfile::Release),
            DesktopJniStripMode::Automatic
        );
        assert_eq!(
            desktop_jni_strip_mode(&config(true), &CargoBuildProfile::Release),
            DesktopJniStripMode::Automatic
        );
        assert_eq!(
            desktop_jni_strip_mode(&config(true), &CargoBuildProfile::Named("dist".to_string())),
            DesktopJniStripMode::Explicit
        );
        assert_eq!(
            desktop_jni_strip_mode(&config(false), &CargoBuildProfile::Debug),
            DesktopJniStripMode::Disabled
        );
    }

    #[test]
    fn rejects_explicit_windows_symbol_stripping_opt_in() {
        let error =
            validate_desktop_jni_symbol_stripping(&config(true), JavaHostTarget::WindowsX86_64)
                .expect_err("windows strip opt-in should fail");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("strip_symbols")
                    && command.contains("windows-x86_64")
        ));
    }

    #[test]
    fn allows_automatic_windows_release_behavior_without_strip_opt_in() {
        validate_desktop_jni_symbol_stripping(&config(false), JavaHostTarget::WindowsX86_64)
            .expect("windows release packaging should remain unchanged without explicit opt-in");
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
    fn resolves_homebrew_java_home_layout_for_host_headers() {
        let temp_root = temporary_directory("boltffi-java-homebrew-layout-test");
        let homebrew_prefix = temp_root.join("openjdk@17");
        let resolved_home = homebrew_prefix
            .join("libexec")
            .join("openjdk.jdk")
            .join("Contents")
            .join("Home");
        let include_root = resolved_home.join("include");
        let platform_include = include_root.join("darwin");
        fs::create_dir_all(&platform_include).expect("create homebrew include dirs");
        fs::write(include_root.join("jni.h"), []).expect("write jni.h");
        fs::write(platform_include.join("jni_md.h"), []).expect("write jni_md.h");

        let include_directories = resolve_jni_include_directories_with_overrides(
            &cargo_context(&temp_root, JavaHostTarget::DarwinArm64),
            Some(homebrew_prefix),
            None,
            None,
        )
        .expect("homebrew java layout should resolve");

        assert_eq!(include_directories.shared, include_root);
        assert_eq!(include_directories.platform, platform_include);

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
