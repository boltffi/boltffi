use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::pack::PackError;
use crate::pack::java::outputs::remove_file_if_exists;
use crate::pack::symbols::{DebugSymbolArtifact, DebugSymbolArtifactKind, write_debug_symbols_zip};
use crate::target::{BuiltLibrary, Platform, RustTarget};
use crate::toolchain::AndroidToolchain;

use super::AndroidBindingMode;

pub struct AndroidPackager<'a> {
    config: &'a Config,
    libraries: Vec<BuiltLibrary>,
    release: bool,
    binding_mode: AndroidBindingMode,
    layout: AndroidPackageLayout,
    scope: AndroidPackScope,
}

/// How much of the configured Android ABI set one packaging run covers. Both
/// cleanups a run does, of jniLibs and of debug symbol archives, follow from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidPackScope {
    /// Every configured ABI. Linking fewer is an error, the run writes the
    /// combined debug symbol archive, and per-ABI archives are stale.
    Complete,
    /// Some of the configured ABIs (`--architecture`). The configured ABIs this
    /// run leaves out stay in jniLibs, and each linked ABI gets its own debug
    /// symbol archive.
    Slice,
}

impl AndroidPackScope {
    /// `Complete` when `targets` covers every configured ABI, so naming all of
    /// them with `--architecture` packs the same way as naming none.
    pub(crate) fn of(config: &Config, targets: &[RustTarget]) -> Self {
        if config
            .android_targets()
            .iter()
            .all(|configured| targets.contains(configured))
        {
            Self::Complete
        } else {
            Self::Slice
        }
    }
}

/// Paths used while compiling and staging Android JNI libraries.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct AndroidPackageLayout {
    pub(crate) jni_glue_path: PathBuf,
    pub(crate) header_include_dir: PathBuf,
    pub(crate) header_name: String,
    pub(crate) jnilibs_path: PathBuf,
}

pub struct AndroidOutput {
    /// ABIs whose library a slice left in jniLibs from an earlier run, so it was
    /// not relinked against the JNI glue this run generated.
    pub(crate) kept_abis: Vec<&'static str>,
}

struct AndroidLinkedOutput {
    target: RustTarget,
    abi: &'static str,
    path: PathBuf,
}

impl<'a> AndroidPackager<'a> {
    /// Builds an Android packager for the standalone Kotlin Android output layout.
    pub fn new(config: &'a Config, libraries: Vec<BuiltLibrary>, release: bool) -> Self {
        let layout = AndroidPackageLayout::kotlin(config);
        Self::new_with_layout(
            config,
            libraries,
            release,
            AndroidBindingMode::Kotlin,
            layout,
        )
    }

    /// Builds an Android packager with an explicit layout supplied by the owning package flow.
    pub(crate) fn new_with_layout(
        config: &'a Config,
        libraries: Vec<BuiltLibrary>,
        release: bool,
        binding_mode: AndroidBindingMode,
        layout: AndroidPackageLayout,
    ) -> Self {
        Self {
            config,
            libraries,
            release,
            binding_mode,
            layout,
            scope: AndroidPackScope::Complete,
        }
    }

    /// Packs only part of the configured ABIs when `scope` is a slice; a packager
    /// covers every configured ABI otherwise.
    pub(crate) fn with_scope(mut self, scope: AndroidPackScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn package(self) -> Result<AndroidOutput> {
        let android_libs = self.filter_android_libraries();

        if android_libs.is_empty() {
            return Err(PackError::NoLibrariesFound {
                platform: "Android".to_string(),
            }
            .into());
        }
        self.ensure_complete_scope_links_every_configured_abi(&android_libs)?;

        let jnilibs_path = self.android_jnilibs_path();
        let android_toolchain = AndroidToolchain::discover(
            self.config.android_min_sdk(),
            self.config.android_ndk_version(),
        )?;

        std::fs::create_dir_all(&jnilibs_path).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: jnilibs_path.clone(),
                source,
            }
        })?;

        let jni_glue_path = self.android_jni_glue_path()?;
        let header_include_dir = self.layout.header_include_dir.clone();
        let header_path = header_include_dir.join(format!("{}.h", self.layout.header_name));
        if !header_path.exists() {
            return Err(CliError::FileNotFound(header_path));
        }

        let mut linked_outputs = Vec::with_capacity(android_libs.len());
        for lib in &android_libs {
            linked_outputs.push(self.link_shared_library(
                lib,
                &jnilibs_path,
                &android_toolchain,
                &jni_glue_path,
                &header_include_dir,
            )?);
        }

        if self.android_debug_symbols_archive_enabled() {
            write_android_debug_symbols(self.config, self.scope, &linked_outputs)?;
        }

        self.remove_stale_packaged_libraries(&jnilibs_path)?;

        Ok(AndroidOutput {
            kept_abis: self.kept_abis(&jnilibs_path, &android_libs),
        })
    }

    /// A complete run that links fewer ABIs than configured would quietly pack
    /// like a slice, keeping the missing ABIs' old libraries, so it is refused.
    fn ensure_complete_scope_links_every_configured_abi(
        &self,
        android_libs: &[&BuiltLibrary],
    ) -> Result<()> {
        if self.scope == AndroidPackScope::Slice {
            return Ok(());
        }

        let missing_targets: Vec<_> = self
            .config
            .android_targets()
            .into_iter()
            .filter(|target| !android_libs.iter().any(|library| library.target == *target))
            .map(|target| target.triple().to_string())
            .collect();
        if !missing_targets.is_empty() {
            return Err(PackError::MissingBuiltLibraries {
                platform: "Android".to_string(),
                targets: missing_targets,
            }
            .into());
        }

        Ok(())
    }

    /// Only an ABI the configuration no longer carries is stale. A configured ABI
    /// a slice does not package belongs to another slice packed into the same
    /// output, and deleting it would undo that run's work.
    fn remove_stale_packaged_libraries(&self, jnilibs_path: &Path) -> Result<()> {
        let configured = self.config.android_targets();
        let lib_file_name = self.android_library_file_name();

        RustTarget::ALL_ANDROID
            .iter()
            .filter(|target| !configured.contains(target))
            .try_for_each(|target| {
                remove_file_if_exists(
                    &jnilibs_path
                        .join(target.architecture().android_abi())
                        .join(&lib_file_name),
                )
            })
    }

    /// The configured ABIs a slice did not link but whose library an earlier run
    /// left in jniLibs.
    fn kept_abis(&self, jnilibs_path: &Path, android_libs: &[&BuiltLibrary]) -> Vec<&'static str> {
        let lib_file_name = self.android_library_file_name();

        self.config
            .android_targets()
            .into_iter()
            .filter(|target| !android_libs.iter().any(|library| library.target == *target))
            .map(|target| target.architecture().android_abi())
            .filter(|abi| jnilibs_path.join(abi).join(&lib_file_name).exists())
            .collect()
    }

    fn filter_android_libraries(&self) -> Vec<&BuiltLibrary> {
        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::Android)
            .collect()
    }

    fn android_jnilibs_path(&self) -> PathBuf {
        self.layout.jnilibs_path.clone()
    }

    fn android_jni_glue_path(&self) -> Result<PathBuf> {
        let jni_glue_path = self.layout.jni_glue_path.clone();
        jni_glue_path
            .exists()
            .then_some(jni_glue_path.clone())
            .ok_or(CliError::FileNotFound(jni_glue_path))
    }

    fn link_shared_library(
        &self,
        library: &BuiltLibrary,
        jnilibs_path: &Path,
        android_toolchain: &AndroidToolchain,
        jni_glue_path: &Path,
        header_include_dir: &Path,
    ) -> Result<AndroidLinkedOutput> {
        let abi = library.target.architecture().android_abi();
        let abi_dir = jnilibs_path.join(abi);

        std::fs::create_dir_all(&abi_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: abi_dir.clone(),
            source,
        })?;

        let lib_name = self.android_library_name();
        let dest_path = abi_dir.join(format!("lib{}.so", lib_name));
        let build_dir = PathBuf::from("target")
            .join("boltffi")
            .join("android")
            .join(library.target.triple())
            .join(if self.release { "release" } else { "debug" });
        std::fs::create_dir_all(&build_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: build_dir.clone(),
            source,
        })?;

        let clang = android_toolchain.clang_for_target(&library.target)?;
        let object_path = build_dir.join("jni_glue.o");
        let export_script_path = build_dir.join("exports.map");

        let mut compile = Command::new(&clang);
        compile.args(android_jni_compile_args(
            &object_path,
            header_include_dir,
            jni_glue_path,
            self.release,
            self.android_debug_symbols_enabled(),
        ));
        run_command(compile)?;

        write_android_export_version_script(&export_script_path)?;

        let mut link = Command::new(&clang);
        link.args(android_shared_link_args(
            &dest_path,
            &object_path,
            &library.path,
            &export_script_path,
            self.config.android_extra_link_args(),
        ));
        run_command(link)?;

        Ok(AndroidLinkedOutput {
            target: library.target,
            abi,
            path: dest_path,
        })
    }

    fn android_library_name(&self) -> String {
        self.config.resolved_android_kotlin_library_name()
    }

    fn android_library_file_name(&self) -> String {
        format!("lib{}.so", self.android_library_name())
    }

    fn android_debug_symbols_enabled(&self) -> bool {
        matches!(self.binding_mode, AndroidBindingMode::Kotlin)
            && self.config.android_debug_symbols_enabled()
    }

    fn android_debug_symbols_archive_enabled(&self) -> bool {
        matches!(self.binding_mode, AndroidBindingMode::Kotlin)
            && self.config.android_debug_symbols_archive_enabled()
    }
}

impl AndroidPackageLayout {
    /// Builds the default layout for standalone Kotlin Android packaging.
    pub(crate) fn kotlin(config: &Config) -> Self {
        Self {
            jni_glue_path: config
                .android_kotlin_output()
                .join("jni")
                .join("jni_glue.c"),
            header_include_dir: config.android_header_output(),
            header_name: config.library_name().to_string(),
            jnilibs_path: config.android_pack_output(),
        }
    }
}

fn android_jni_compile_args(
    object_path: &Path,
    header_include_dir: &Path,
    jni_glue_path: &Path,
    release: bool,
    emit_debug_info: bool,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-c"),
        OsString::from("-fPIC"),
        OsString::from(if release { "-O3" } else { "-O0" }),
    ];
    if emit_debug_info {
        args.push(OsString::from("-g"));
    }
    args.extend([
        OsString::from("-I"),
        header_include_dir.as_os_str().to_os_string(),
        jni_glue_path.as_os_str().to_os_string(),
        OsString::from("-o"),
        object_path.as_os_str().to_os_string(),
    ]);
    args
}

/// The debug symbol archive name for one ABI, or for the combined archive
/// covering every configured ABI when `abi` is `None`.
fn android_debug_symbols_archive_name(config: &Config, abi: Option<&str>) -> String {
    let crate_artifact_name = config.crate_artifact_name();
    match config.android_debug_symbols_format() {
        crate::config::DebugSymbolsFormat::Zip => match abi {
            Some(abi) => format!("{crate_artifact_name}.android.{abi}.symbols.zip"),
            None => format!("{crate_artifact_name}.android.symbols.zip"),
        },
    }
}

/// Writes the debug symbol archives for this run's linked libraries.
///
/// A complete run writes the one combined archive, and any per-ABI archive an
/// earlier slice left behind predates it. A slice writes one archive per linked
/// ABI, so slices packed into the same output keep each other's archives. The
/// combined archive no longer matches the ABIs a slice relinks, and a per-ABI
/// archive for an ABI the configuration dropped has no library left in jniLibs,
/// so a slice removes both.
fn write_android_debug_symbols(
    config: &Config,
    scope: AndroidPackScope,
    linked_outputs: &[AndroidLinkedOutput],
) -> Result<Vec<PathBuf>> {
    let output_dir = config.android_debug_symbols_output();
    let configured = config.android_targets();
    let per_abi_archive = |target: &RustTarget| {
        output_dir.join(android_debug_symbols_archive_name(
            config,
            Some(target.architecture().android_abi()),
        ))
    };

    match scope {
        AndroidPackScope::Complete => {
            RustTarget::ALL_ANDROID
                .iter()
                .try_for_each(|target| remove_file_if_exists(&per_abi_archive(target)))?;
            Ok(vec![write_android_debug_symbols_archive(
                config,
                &android_debug_symbols_archive_name(config, None),
                linked_outputs,
            )?])
        }
        AndroidPackScope::Slice => {
            remove_file_if_exists(
                &output_dir.join(android_debug_symbols_archive_name(config, None)),
            )?;
            RustTarget::ALL_ANDROID
                .iter()
                .filter(|target| !configured.contains(target))
                .try_for_each(|target| remove_file_if_exists(&per_abi_archive(target)))?;
            linked_outputs
                .iter()
                .map(|output| {
                    write_android_debug_symbols_archive(
                        config,
                        &android_debug_symbols_archive_name(config, Some(output.abi)),
                        std::slice::from_ref(output),
                    )
                })
                .collect()
        }
    }
}

fn write_android_debug_symbols_archive(
    config: &Config,
    archive_name: &str,
    linked_outputs: &[AndroidLinkedOutput],
) -> Result<PathBuf> {
    let artifacts = linked_outputs
        .iter()
        .map(|output| DebugSymbolArtifact {
            source_path: output.path.clone(),
            archive_path: PathBuf::from("jniLibs").join(output.abi).join(
                output
                    .path
                    .file_name()
                    .expect("android library should have a filename"),
            ),
            kind: DebugSymbolArtifactKind::Shared,
            target_triple: Some(output.target.triple().to_string()),
            platform: Some(output.target.platform()),
            architecture: Some(output.target.architecture()),
            abi: Some(output.abi.to_string()),
            host_target: None,
        })
        .collect::<Vec<_>>();

    write_debug_symbols_zip(
        &config.android_debug_symbols_output(),
        archive_name,
        "android",
        match config.android_debug_symbols_bundle() {
            crate::config::DebugSymbolsBundle::Unstripped => "unstripped",
        },
        &artifacts,
    )
}

fn android_shared_link_args(
    dest_path: &Path,
    object_path: &Path,
    library_path: &Path,
    export_script_path: &Path,
    extra_args: &[String],
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-shared"),
        OsString::from("-o"),
        dest_path.as_os_str().to_os_string(),
        object_path.as_os_str().to_os_string(),
        OsString::from("-Wl,--whole-archive"),
        library_path.as_os_str().to_os_string(),
        OsString::from("-Wl,--no-whole-archive"),
        OsString::from("-Xlinker"),
        OsString::from("--version-script"),
        OsString::from("-Xlinker"),
        export_script_path.as_os_str().to_os_string(),
        OsString::from("-Wl,--gc-sections"),
        OsString::from("-Wl,-z,nodelete"),
        OsString::from("-lm"),
        OsString::from("-llog"),
        OsString::from("-ldl"),
    ];
    args.extend(extra_args.iter().map(OsString::from));
    args
}

fn write_android_export_version_script(path: &Path) -> Result<()> {
    std::fs::write(path, android_export_version_script()).map_err(|source| {
        CliError::CommandFailed {
            command: format!("write android linker version script {}", path.display()),
            status: source.raw_os_error(),
        }
    })
}

fn android_export_version_script() -> &'static str {
    r#"{
    global:
        Java_*;
        JNI_OnLoad*;
        JNI_OnUnload*;
        boltffi_*;
    local:
        *;
};
"#
}

fn run_command(mut command: Command) -> Result<()> {
    let command_string = format!("{:?}", command);
    let status = command.status().map_err(|_| CliError::CommandFailed {
        command: command_string.clone(),
        status: None,
    })?;

    status
        .success()
        .then_some(())
        .ok_or(CliError::CommandFailed {
            command: command_string,
            status: status.code(),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        AndroidLinkedOutput, AndroidPackScope, AndroidPackageLayout, AndroidPackager,
        android_export_version_script, android_jni_compile_args, android_shared_link_args,
        write_android_debug_symbols,
    };
    use crate::cli::CliError;
    use crate::config::Config;
    use crate::pack::PackError;
    use crate::pack::android::AndroidBindingMode;
    use crate::target::{BuiltLibrary, RustTarget};
    use std::ffi::OsString;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn kmp_android_layout(output_root: &Path) -> AndroidPackageLayout {
        AndroidPackageLayout {
            jni_glue_path: output_root.join("src/androidMain/c/jni_glue.c"),
            header_include_dir: output_root.join("src/androidMain/c"),
            header_name: "demo".to_string(),
            jnilibs_path: output_root.join("src/androidMain/jniLibs"),
        }
    }

    #[test]
    fn stale_cleanup_removes_only_boltffi_library_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("boltffi-android-packager-test-{unique}"));
        let pack_output = root.join("jniLibs");
        let config = parse_config(&format!(
            r#"
[package]
name = "demo"

[targets.android]
architectures = ["arm64"]

[targets.android.pack]
output = "{}"
"#,
            pack_output.display()
        ));

        let stale_abi_dir = pack_output.join("x86");
        fs::create_dir_all(&stale_abi_dir).expect("create stale abi dir");
        let stale_boltffi = stale_abi_dir.join("libdemo.so");
        let unrelated = stale_abi_dir.join("libdependency.so");
        fs::write(&stale_boltffi, []).expect("write stale boltffi lib");
        fs::write(&unrelated, []).expect("write unrelated lib");

        let arm64_abi_dir = pack_output.join("arm64-v8a");
        fs::create_dir_all(&arm64_abi_dir).expect("create configured abi dir");
        let packager = AndroidPackager::new(
            &config,
            vec![BuiltLibrary {
                target: RustTarget::ANDROID_ARM64,
                path: root.join("libdemo.a"),
            }],
            false,
        );
        packager
            .remove_stale_packaged_libraries(&pack_output)
            .expect("cleanup succeeds");

        assert!(!stale_boltffi.exists());
        assert!(unrelated.exists());

        fs::remove_dir_all(&root).expect("cleanup temp dir");
    }

    fn write_abi_library(pack_output: &Path, abi: &str) -> PathBuf {
        let abi_dir = pack_output.join(abi);
        fs::create_dir_all(&abi_dir).expect("create abi dir");
        let library = abi_dir.join("libdemo.so");
        fs::write(&library, abi).expect("write abi library");
        library
    }

    fn built_library(target: RustTarget) -> BuiltLibrary {
        BuiltLibrary {
            target,
            path: PathBuf::from("libdemo.a"),
        }
    }

    fn arm64_and_x86_64_pack_config(pack_output: &Path) -> Config {
        parse_config(&format!(
            r#"
[package]
name = "demo"

[targets.android]
architectures = ["arm64", "x86_64"]

[targets.android.pack]
output = "{}"
"#,
            pack_output.display()
        ))
    }

    #[test]
    fn android_pack_scope_is_complete_only_when_every_configured_abi_is_selected() {
        let config = arm64_and_x86_64_pack_config(Path::new("jniLibs"));

        assert_eq!(
            AndroidPackScope::of(&config, &config.android_targets()),
            AndroidPackScope::Complete
        );
        assert_eq!(
            AndroidPackScope::of(
                &config,
                &[RustTarget::ANDROID_X86_64, RustTarget::ANDROID_ARM64]
            ),
            AndroidPackScope::Complete
        );
        assert_eq!(
            AndroidPackScope::of(&config, &[RustTarget::ANDROID_ARM64]),
            AndroidPackScope::Slice
        );
    }

    /// Two `--architecture` slices packed one after the other into the same
    /// output must both survive: the second run sweeps only ABIs the
    /// configuration dropped, not the configured one the first run packaged,
    /// and reports the one it kept without relinking.
    #[test]
    fn slice_keeps_configured_abis_packaged_by_another_run_and_reports_them() {
        let root = tempfile::tempdir().expect("create temp dir");
        let pack_output = root.path().join("jniLibs");
        let config = arm64_and_x86_64_pack_config(&pack_output);

        let arm64_from_earlier_run = write_abi_library(&pack_output, "arm64-v8a");
        let x86_64_from_this_run = write_abi_library(&pack_output, "x86_64");
        let dropped_from_config = write_abi_library(&pack_output, "armeabi-v7a");

        let packager = AndroidPackager::new(
            &config,
            vec![built_library(RustTarget::ANDROID_X86_64)],
            false,
        )
        .with_scope(AndroidPackScope::Slice);
        packager
            .remove_stale_packaged_libraries(&pack_output)
            .expect("cleanup succeeds");

        assert!(arm64_from_earlier_run.exists());
        assert!(x86_64_from_this_run.exists());
        assert!(!dropped_from_config.exists());
        assert_eq!(
            packager.kept_abis(&pack_output, &packager.filter_android_libraries()),
            vec!["arm64-v8a"]
        );
    }

    #[test]
    fn complete_scope_refuses_to_link_fewer_abis_than_configured() {
        let config = arm64_and_x86_64_pack_config(Path::new("jniLibs"));
        let arm64_only = [built_library(RustTarget::ANDROID_ARM64)];
        let arm64_only = arm64_only.iter().collect::<Vec<_>>();

        let complete = AndroidPackager::new(&config, Vec::new(), false);
        let error = complete
            .ensure_complete_scope_links_every_configured_abi(&arm64_only)
            .unwrap_err();
        assert!(matches!(
            error,
            CliError::Pack(PackError::MissingBuiltLibraries { ref targets, .. })
                if targets == &["x86_64-linux-android".to_string()]
        ));

        let slice =
            AndroidPackager::new(&config, Vec::new(), false).with_scope(AndroidPackScope::Slice);
        assert!(
            slice
                .ensure_complete_scope_links_every_configured_abi(&arm64_only)
                .is_ok()
        );
    }

    fn debug_symbols_config(symbols_output: &Path) -> Config {
        parse_config(&format!(
            r#"
[package]
name = "demo"

[targets.android]
architectures = ["arm64", "x86_64"]

[targets.android.debug_symbols]
enabled = true
output = "{}"
"#,
            symbols_output.display()
        ))
    }

    fn linked_output(root: &Path, target: RustTarget) -> AndroidLinkedOutput {
        let abi = target.architecture().android_abi();
        AndroidLinkedOutput {
            target,
            abi,
            path: write_abi_library(&root.join("linked"), abi),
        }
    }

    /// The `.so` entries and the ABIs `symbols.json` lists, both sorted.
    fn archive_contents(archive: &Path) -> (Vec<String>, Vec<String>) {
        let file = fs::File::open(archive).expect("open symbols archive");
        let mut archive = zip::ZipArchive::new(file).expect("read symbols archive");
        let mut libraries = Vec::new();
        let mut manifest_abis = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("archive entry");
            let name = entry.name().to_string();
            if name.ends_with(".so") {
                libraries.push(name);
            } else if name.ends_with("symbols.json") {
                let manifest: serde_json::Value =
                    serde_json::from_reader(&mut entry).expect("parse symbols.json");
                manifest_abis.extend(
                    manifest["artifacts"]
                        .as_array()
                        .expect("manifest artifacts")
                        .iter()
                        .map(|artifact| artifact["abi"].as_str().expect("abi").to_string()),
                );
            }
        }
        libraries.sort();
        manifest_abis.sort();
        (libraries, manifest_abis)
    }

    #[test]
    fn complete_run_writes_the_combined_archive_and_drops_per_abi_ones() {
        let root = tempfile::tempdir().expect("create temp dir");
        let symbols_output = root.path().join("symbols");
        let config = debug_symbols_config(&symbols_output);
        fs::create_dir_all(&symbols_output).expect("create symbols dir");
        let stale_slice_archive = symbols_output.join("demo.android.arm64-v8a.symbols.zip");
        let dropped_abi_archive = symbols_output.join("demo.android.x86.symbols.zip");
        fs::write(&stale_slice_archive, []).expect("write stale slice archive");
        fs::write(&dropped_abi_archive, []).expect("write dropped abi archive");

        let outputs = [
            linked_output(root.path(), RustTarget::ANDROID_ARM64),
            linked_output(root.path(), RustTarget::ANDROID_X86_64),
        ];
        let archives = write_android_debug_symbols(&config, AndroidPackScope::Complete, &outputs)
            .expect("write symbols");

        assert_eq!(
            archives,
            vec![symbols_output.join("demo.android.symbols.zip")]
        );
        assert_eq!(
            archive_contents(&archives[0]),
            (
                vec![
                    "demo.android.symbols/jniLibs/arm64-v8a/libdemo.so".to_string(),
                    "demo.android.symbols/jniLibs/x86_64/libdemo.so".to_string(),
                ],
                vec!["arm64-v8a".to_string(), "x86_64".to_string()]
            )
        );
        assert!(!stale_slice_archive.exists());
        assert!(!dropped_abi_archive.exists());
    }

    /// Slices packed into the same output write one archive per ABI, so the
    /// second slice leaves the first slice's archive in place. The combined
    /// archive and a dropped ABI's archive no longer match jniLibs, so they go.
    #[test]
    fn slices_write_one_archive_per_abi_and_drop_the_stale_ones() {
        let root = tempfile::tempdir().expect("create temp dir");
        let symbols_output = root.path().join("symbols");
        let config = debug_symbols_config(&symbols_output);
        fs::create_dir_all(&symbols_output).expect("create symbols dir");
        let combined_archive = symbols_output.join("demo.android.symbols.zip");
        let dropped_abi_archive = symbols_output.join("demo.android.x86.symbols.zip");
        fs::write(&combined_archive, []).expect("write combined archive");
        fs::write(&dropped_abi_archive, []).expect("write dropped abi archive");

        let arm64 = write_android_debug_symbols(
            &config,
            AndroidPackScope::Slice,
            &[linked_output(root.path(), RustTarget::ANDROID_ARM64)],
        )
        .expect("write arm64 symbols");
        let x86_64 = write_android_debug_symbols(
            &config,
            AndroidPackScope::Slice,
            &[linked_output(root.path(), RustTarget::ANDROID_X86_64)],
        )
        .expect("write x86_64 symbols");

        assert_eq!(
            arm64,
            vec![symbols_output.join("demo.android.arm64-v8a.symbols.zip")]
        );
        assert_eq!(
            x86_64,
            vec![symbols_output.join("demo.android.x86_64.symbols.zip")]
        );
        assert_eq!(
            archive_contents(&arm64[0]),
            (
                vec!["demo.android.arm64-v8a.symbols/jniLibs/arm64-v8a/libdemo.so".to_string()],
                vec!["arm64-v8a".to_string()]
            )
        );
        assert_eq!(
            archive_contents(&x86_64[0]),
            (
                vec!["demo.android.x86_64.symbols/jniLibs/x86_64/libdemo.so".to_string()],
                vec!["x86_64".to_string()]
            )
        );
        assert!(!combined_archive.exists());
        assert!(!dropped_abi_archive.exists());
    }

    #[test]
    fn android_jni_glue_path_uses_kotlin_output_for_legacy_android_bindings() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("boltffi-android-jni-path-test-{unique}"));
        let kotlin_output = root.join("kotlin");
        let expected_glue = kotlin_output.join("jni/jni_glue.c");
        fs::create_dir_all(expected_glue.parent().expect("jni glue parent"))
            .expect("create kotlin jni dir");
        fs::write(&expected_glue, []).expect("write kotlin jni glue");

        let config = parse_config(&format!(
            r#"
[package]
name = "demo"

[targets.android.kotlin]
output = "{}"
"#,
            kotlin_output.display()
        ));
        let packager = AndroidPackager::new(&config, Vec::new(), false);

        assert_eq!(
            packager.android_jni_glue_path().expect("jni glue path"),
            expected_glue
        );

        fs::remove_dir_all(&root).expect("cleanup temp dir");
    }

    #[test]
    fn android_jnilibs_path_uses_android_pack_output_for_legacy_android_bindings_even_with_kmp() {
        let root = std::env::temp_dir().join("boltffi-android-jni-output-test");
        let pack_output = root.join("configured-jniLibs");
        let kmp_output = root.join("kmp");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.android.pack]
output = "{}"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            pack_output.display(),
            kmp_output.display()
        ));
        let packager = AndroidPackager::new(&config, Vec::new(), false);

        assert_eq!(packager.android_jnilibs_path(), pack_output);
    }

    #[test]
    fn android_library_name_uses_kotlin_loader_override() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.android.kotlin]
library_name = "configured-library"
"#,
        );
        let legacy_packager = AndroidPackager::new(&config, Vec::new(), false);
        let kmp_packager = AndroidPackager::new_with_layout(
            &config,
            Vec::new(),
            false,
            AndroidBindingMode::KotlinMultiplatform,
            kmp_android_layout(Path::new("kmp")),
        );

        assert_eq!(legacy_packager.android_library_name(), "configured-library");
        assert_eq!(kmp_packager.android_library_name(), "configured-library");
    }

    #[test]
    fn android_jni_glue_path_uses_kmp_android_glue_for_kmp_bindings() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("boltffi-android-kmp-jni-path-test-{unique}"));
        let kmp_output = root.join("kmp");
        let expected_glue = kmp_output.join("src/androidMain/c/jni_glue.c");
        fs::create_dir_all(expected_glue.parent().expect("jni glue parent"))
            .expect("create kmp jni dir");
        fs::write(&expected_glue, []).expect("write kmp jni glue");

        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            kmp_output.display()
        ));
        let packager = AndroidPackager::new_with_layout(
            &config,
            Vec::new(),
            false,
            AndroidBindingMode::KotlinMultiplatform,
            kmp_android_layout(&kmp_output),
        );

        assert_eq!(
            packager.android_jni_glue_path().expect("jni glue path"),
            expected_glue
        );

        fs::remove_dir_all(&root).expect("cleanup temp dir");
    }

    #[test]
    fn android_jnilibs_path_uses_kmp_android_main_jnilibs_for_kmp_bindings() {
        let root = std::env::temp_dir().join("boltffi-android-kmp-jni-output-test");
        let kmp_output = root.join("kmp");
        let android_pack_output = root.join("legacy-jniLibs");
        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.android.pack]
output = "{}"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            android_pack_output.display(),
            kmp_output.display()
        ));
        let packager = AndroidPackager::new_with_layout(
            &config,
            Vec::new(),
            false,
            AndroidBindingMode::KotlinMultiplatform,
            kmp_android_layout(&kmp_output),
        );

        assert_eq!(
            packager.android_jnilibs_path(),
            kmp_output.join("src/androidMain/jniLibs")
        );
    }

    #[test]
    fn kmp_android_packager_uses_explicit_layout_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("boltffi-android-kmp-explicit-layout-test-{unique}"));
        let config_kmp_output = root.join("config-kmp");
        let layout_kmp_output = root.join("layout-kmp");
        let expected_glue = layout_kmp_output.join("src/androidMain/c/jni_glue.c");
        let expected_jnilibs = layout_kmp_output.join("src/androidMain/jniLibs");
        fs::create_dir_all(expected_glue.parent().expect("jni glue parent"))
            .expect("create layout jni dir");
        fs::write(&expected_glue, []).expect("write layout jni glue");

        let config = parse_config(&format!(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"

[targets.kotlin_multiplatform]
enabled = true
output = "{}"
"#,
            config_kmp_output.display()
        ));
        let layout = AndroidPackageLayout {
            jni_glue_path: expected_glue.clone(),
            header_include_dir: layout_kmp_output.join("src/androidMain/c"),
            header_name: "demo".to_string(),
            jnilibs_path: expected_jnilibs.clone(),
        };
        let packager = AndroidPackager::new_with_layout(
            &config,
            Vec::new(),
            false,
            AndroidBindingMode::KotlinMultiplatform,
            layout,
        );

        assert_eq!(
            packager.android_jni_glue_path().expect("jni glue path"),
            expected_glue
        );
        assert_eq!(packager.android_jnilibs_path(), expected_jnilibs);

        fs::remove_dir_all(&root).expect("cleanup temp dir");
    }

    #[test]
    fn kmp_android_packaging_does_not_write_legacy_debug_symbols() {
        let config = parse_config(
            r#"
[package]
name = "demo"

[targets.android.debug_symbols]
enabled = true
"#,
        );
        let legacy_packager = AndroidPackager::new(&config, Vec::new(), false);
        let kmp_packager = AndroidPackager::new_with_layout(
            &config,
            Vec::new(),
            false,
            AndroidBindingMode::KotlinMultiplatform,
            kmp_android_layout(Path::new("kmp")),
        );

        assert!(legacy_packager.android_debug_symbols_enabled());
        assert!(!kmp_packager.android_debug_symbols_enabled());
    }

    #[test]
    fn android_linker_uses_export_map_and_collects_unused_sections() {
        let args = android_shared_link_args(
            Path::new("/tmp/out/libdemo.so"),
            Path::new("/tmp/out/jni_glue.o"),
            Path::new("/tmp/out/libdemo.a"),
            Path::new("/tmp/out/exports.map"),
            &[],
        );

        assert!(!args.contains(&OsString::from("-Wl,--exclude-libs,ALL")));
        assert!(args.contains(&OsString::from("--version-script")));
        assert!(args.contains(&OsString::from("/tmp/out/exports.map")));
        assert!(args.contains(&OsString::from("-Wl,--gc-sections")));
    }

    #[test]
    fn android_linker_marks_jni_library_nodelete_for_cached_thread_destructors() {
        let args = android_shared_link_args(
            Path::new("/tmp/out/libdemo.so"),
            Path::new("/tmp/out/jni_glue.o"),
            Path::new("/tmp/out/libdemo.a"),
            Path::new("/tmp/out/exports.map"),
            &[],
        );

        assert!(
            args.contains(&OsString::from("-Wl,-z,nodelete")),
            "Android JNI libraries should stay mapped so pthread TLS destructors remain callable"
        );
    }

    #[test]
    fn android_export_map_keeps_public_jni_and_boltffi_symbols() {
        let script = android_export_version_script();

        assert!(script.contains("Java_*;"));
        assert!(script.contains("JNI_OnLoad*;"));
        assert!(script.contains("JNI_OnUnload*;"));
        assert!(script.contains("boltffi_*;"));
        assert!(script.contains("local:"));
        assert!(script.contains("*;"));
    }

    #[test]
    fn android_jni_compile_args_include_debug_info_when_requested() {
        let args = android_jni_compile_args(
            Path::new("/tmp/out/jni_glue.o"),
            Path::new("/tmp/include"),
            Path::new("/tmp/jni/jni_glue.c"),
            true,
            true,
        );

        assert!(args.contains(&OsString::from("-g")));
    }

    #[cfg(unix)]
    #[test]
    fn android_linker_preserves_non_utf8_paths() {
        let dest_path = PathBuf::from(OsString::from_vec(b"/tmp/out-\xFF.so".to_vec()));
        let object_path = PathBuf::from(OsString::from_vec(b"/tmp/jni-\xFE.o".to_vec()));
        let library_path = PathBuf::from(OsString::from_vec(b"/tmp/lib-\xFD.a".to_vec()));
        let export_script_path =
            PathBuf::from(OsString::from_vec(b"/tmp/exports-\xFC.map".to_vec()));
        let args = android_shared_link_args(
            &dest_path,
            &object_path,
            &library_path,
            &export_script_path,
            &[],
        );

        assert_eq!(
            args[2].as_os_str().as_bytes(),
            dest_path.as_os_str().as_bytes()
        );
        assert_eq!(
            args[3].as_os_str().as_bytes(),
            object_path.as_os_str().as_bytes()
        );
        assert_eq!(
            args[5].as_os_str().as_bytes(),
            library_path.as_os_str().as_bytes()
        );
        assert_eq!(
            args[10].as_os_str().as_bytes(),
            export_script_path.as_os_str().as_bytes()
        );
    }

    #[test]
    fn android_linker_appends_configured_extra_link_args() {
        let extra_args = vec!["-Wl,-z,max-page-size=16384".to_string()];
        let args = android_shared_link_args(
            Path::new("/tmp/out/libdemo.so"),
            Path::new("/tmp/out/jni_glue.o"),
            Path::new("/tmp/out/libdemo.a"),
            Path::new("/tmp/out/exports.map"),
            &extra_args,
        );

        assert!(args.contains(&OsString::from("-Wl,-z,max-page-size=16384")));
        assert_eq!(
            args.last(),
            Some(&OsString::from("-Wl,-z,max-page-size=16384")),
            "extra link args should be appended after the built-in flags so they can override them"
        );
    }
}
