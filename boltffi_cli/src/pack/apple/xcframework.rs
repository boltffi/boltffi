use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use askama::Template;

use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::pack::PackError;
use crate::target::{BuiltLibrary, Platform};

use super::names::AppleNames;

pub struct XcframeworkBuilder<'a> {
    config: &'a Config,
    names: AppleNames,
    libraries: Vec<BuiltLibrary>,
    headers_dir: PathBuf,
    output_dir: PathBuf,
    scratch_dir: PathBuf,
}

pub struct XcframeworkOutput {
    pub xcframework_path: PathBuf,
    pub zip_path: Option<PathBuf>,
    pub checksum: Option<String>,
}

impl<'a> XcframeworkBuilder<'a> {
    pub fn new(
        config: &'a Config,
        libraries: Vec<BuiltLibrary>,
        headers_dir: PathBuf,
        scratch_dir: PathBuf,
    ) -> Self {
        Self {
            config,
            names: AppleNames::from_config(config),
            libraries,
            headers_dir,
            output_dir: config.apple_xcframework_output(),
            scratch_dir,
        }
    }

    pub fn build(self) -> Result<XcframeworkOutput> {
        fs::create_dir_all(&self.output_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: self.output_dir.clone(),
            source,
        })?;
        remove_directory_if_exists(&self.scratch_dir)?;
        fs::create_dir_all(&self.scratch_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: self.scratch_dir.clone(),
                source,
            }
        })?;

        let library_groups = AppleLibraryGroups::from_config(self.config, &self.libraries);
        let library_slices = self.resolve_library_slices(&library_groups)?;
        let plan = XcframeworkPlan::new(
            &self.output_dir,
            &self.scratch_dir,
            &self.names,
            self.headers_dir.clone(),
            library_slices,
        );

        plan.prepare_output()?;
        let library_inputs = plan.create_static_library_inputs()?;
        self.run_create_xcframework(&plan.xcframework_path, &library_inputs)?;

        Ok(XcframeworkOutput {
            xcframework_path: plan.xcframework_path,
            zip_path: None,
            checksum: None,
        })
    }

    pub fn build_with_zip(self) -> Result<XcframeworkOutput> {
        let mut output = self.build()?;

        let zip_path = output.xcframework_path.with_extension("xcframework.zip");
        create_zip(&output.xcframework_path, &zip_path)?;

        let checksum = compute_checksum(&zip_path)?;

        output.zip_path = Some(zip_path);
        output.checksum = Some(checksum);

        Ok(output)
    }

    fn create_fat_library(
        &self,
        libs: &[&BuiltLibrary],
        slice_kind: AppleLibrarySliceKind,
    ) -> Result<Option<PathBuf>> {
        if libs.is_empty() {
            return Ok(None);
        }

        if libs.len() == 1 {
            return Ok(Some(libs[0].path.clone()));
        }

        let fat_dir = self
            .scratch_dir
            .join("fat")
            .join(slice_kind.fat_directory_name());
        fs::create_dir_all(&fat_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: fat_dir.clone(),
            source,
        })?;

        let fat_lib_path = fat_dir.join(format!("lib{}.a", self.names.library_name()));

        let mut lipo_cmd = Command::new("lipo");
        lipo_cmd.arg("-create");

        libs.iter().for_each(|lib| {
            lipo_cmd.arg(&lib.path);
        });

        lipo_cmd.arg("-output").arg(&fat_lib_path);

        let status = lipo_cmd
            .status()
            .map_err(|source| PackError::LipoFailed { source })?;

        if !status.success() {
            return Err(CliError::CommandFailed {
                command: "lipo".to_string(),
                status: status.code(),
            });
        }

        Ok(Some(fat_lib_path))
    }

    fn resolve_library_slices(
        &self,
        groups: &AppleLibraryGroups<'_>,
    ) -> Result<Vec<AppleLibrarySlice>> {
        let mut slices = groups
            .device_libraries
            .iter()
            .map(|library| AppleLibrarySlice::device(library))
            .collect::<Vec<_>>();

        if let Some(library_path) = self.create_fat_library(
            &groups.simulator_libraries,
            AppleLibrarySliceKind::IosSimulator,
        )? {
            slices.push(AppleLibrarySlice::resolved(
                AppleLibrarySliceKind::IosSimulator,
                library_path,
            ));
        }

        if let Some(library_path) =
            self.create_fat_library(&groups.macos_libraries, AppleLibrarySliceKind::MacOs)?
        {
            slices.push(AppleLibrarySlice::resolved(
                AppleLibrarySliceKind::MacOs,
                library_path,
            ));
        }

        Ok(slices)
    }

    fn run_create_xcframework(
        &self,
        xcframework_path: &Path,
        library_inputs: &[XcframeworkLibraryInput],
    ) -> Result<()> {
        let mut xcodebuild_cmd = Command::new("xcodebuild");
        xcodebuild_cmd.arg("-create-xcframework");

        library_inputs.iter().for_each(|input| {
            xcodebuild_cmd
                .arg("-library")
                .arg(input.library_path())
                .arg("-headers")
                .arg(input.headers_path());
        });

        xcodebuild_cmd.arg("-output").arg(xcframework_path);
        xcodebuild_cmd.stdout(Stdio::null());

        let status = xcodebuild_cmd
            .status()
            .map_err(|source| PackError::XcframeworkFailed { source })?;

        if !status.success() {
            return Err(CliError::CommandFailed {
                command: "xcodebuild -create-xcframework".to_string(),
                status: status.code(),
            });
        }

        Ok(())
    }
}

struct AppleLibraryGroups<'a> {
    device_libraries: Vec<&'a BuiltLibrary>,
    simulator_libraries: Vec<&'a BuiltLibrary>,
    macos_libraries: Vec<&'a BuiltLibrary>,
}

impl<'a> AppleLibraryGroups<'a> {
    fn from_config(config: &Config, libraries: &'a [BuiltLibrary]) -> Self {
        Self {
            device_libraries: libraries
                .iter()
                .filter(|library| library.target.platform() == Platform::Ios)
                .collect(),
            simulator_libraries: libraries
                .iter()
                .filter(|library| library.target.platform() == Platform::IosSimulator)
                .collect(),
            macos_libraries: if config.apple_include_macos() {
                libraries
                    .iter()
                    .filter(|library| library.target.platform() == Platform::MacOs)
                    .collect()
            } else {
                Vec::new()
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppleLibrarySliceKind {
    IosDevice,
    IosSimulator,
    MacOs,
}

impl AppleLibrarySliceKind {
    fn fat_directory_name(self) -> &'static str {
        match self {
            Self::IosDevice => "ios-device-fat",
            Self::IosSimulator => "ios-simulator-fat",
            Self::MacOs => "macos-fat",
        }
    }

    fn staging_directory_name(self) -> &'static str {
        match self {
            Self::IosDevice => "ios-device",
            Self::IosSimulator => "ios-simulator",
            Self::MacOs => "macos",
        }
    }
}

#[derive(Debug)]
struct AppleLibrarySlice {
    kind: AppleLibrarySliceKind,
    device_staging_directory_name: Option<String>,
    library_path: PathBuf,
}

impl AppleLibrarySlice {
    fn device(library: &BuiltLibrary) -> Self {
        Self {
            kind: AppleLibrarySliceKind::IosDevice,
            device_staging_directory_name: Some(library.target.triple().to_string()),
            library_path: library.path.clone(),
        }
    }

    fn resolved(kind: AppleLibrarySliceKind, library_path: PathBuf) -> Self {
        Self {
            kind,
            device_staging_directory_name: None,
            library_path,
        }
    }

    fn staging_directory_name(&self) -> &str {
        self.device_staging_directory_name
            .as_deref()
            .unwrap_or_else(|| self.kind.staging_directory_name())
    }
}

struct XcframeworkPlan {
    xcframework_path: PathBuf,
    legacy_output_staging_dirs: Vec<PathBuf>,
    library_staging_dir: PathBuf,
    library_plans: Vec<StaticLibrarySlicePlan>,
}

impl XcframeworkPlan {
    fn new(
        output_dir: &Path,
        scratch_dir: &Path,
        names: &AppleNames,
        headers_dir: PathBuf,
        library_slices: Vec<AppleLibrarySlice>,
    ) -> Self {
        let xcframework_path = output_dir.join(format!("{}.xcframework", names.xcframework_name()));
        let library_staging_dir = scratch_dir.join("libraries");
        let module_name = names.ffi_module_name().to_string();
        let library_name = names.library_name().to_string();
        let legacy_output_staging_dirs = [
            output_dir.join("headers_staging"),
            output_dir.join("framework_staging"),
        ]
        .into_iter()
        .chain(
            [
                AppleLibrarySliceKind::IosDevice,
                AppleLibrarySliceKind::IosSimulator,
                AppleLibrarySliceKind::MacOs,
            ]
            .into_iter()
            .map(|slice_kind| output_dir.join(slice_kind.fat_directory_name())),
        )
        .collect();

        let library_plans = library_slices
            .into_iter()
            .map(|library_slice| {
                let headers_path = library_staging_dir
                    .join(library_slice.staging_directory_name())
                    .join("Headers");

                StaticLibrarySlicePlan::new(
                    headers_path,
                    module_name.clone(),
                    library_name.clone(),
                    headers_dir.clone(),
                    library_slice.library_path,
                )
            })
            .collect();

        Self {
            xcframework_path,
            legacy_output_staging_dirs,
            library_staging_dir,
            library_plans,
        }
    }

    fn prepare_output(&self) -> Result<()> {
        remove_directory_if_exists(&self.xcframework_path)?;
        self.legacy_output_staging_dirs
            .iter()
            .try_for_each(|path| remove_directory_if_exists(path))?;
        remove_directory_if_exists(&self.library_staging_dir)?;

        fs::create_dir_all(&self.library_staging_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: self.library_staging_dir.clone(),
                source,
            }
        })
    }

    fn create_static_library_inputs(&self) -> Result<Vec<XcframeworkLibraryInput>> {
        self.library_plans
            .iter()
            .map(StaticLibrarySlicePlan::execute)
            .collect()
    }
}

struct StaticLibrarySlicePlan {
    headers_path: PathBuf,
    module_name: String,
    library_name: String,
    headers_dir: PathBuf,
    library_path: PathBuf,
    public_header_path: String,
}

impl StaticLibrarySlicePlan {
    fn new(
        headers_path: PathBuf,
        module_name: String,
        library_name: String,
        headers_dir: PathBuf,
        library_path: PathBuf,
    ) -> Self {
        let public_header_path = format!("{}/{}.h", library_name, library_name);

        Self {
            headers_path,
            module_name,
            library_name,
            headers_dir,
            library_path,
            public_header_path,
        }
    }

    fn execute(&self) -> Result<XcframeworkLibraryInput> {
        remove_directory_if_exists(&self.headers_path)?;

        let namespaced_headers_path = self.namespaced_headers_path();

        fs::create_dir_all(&namespaced_headers_path).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: namespaced_headers_path.clone(),
                source,
            }
        })?;

        copy_directory_contents(&self.headers_dir, &namespaced_headers_path)?;
        self.write_modulemap()?;

        Ok(XcframeworkLibraryInput {
            library_path: self.library_path.clone(),
            headers_path: self.headers_path.clone(),
        })
    }

    fn namespaced_headers_path(&self) -> PathBuf {
        self.headers_path.join(&self.library_name)
    }

    fn write_modulemap(&self) -> Result<()> {
        let modulemap_content =
            render_library_modulemap(&self.module_name, &self.public_header_path)?;
        let modulemap_path = self.headers_path.join("module.modulemap");

        fs::write(&modulemap_path, modulemap_content).map_err(|source| CliError::WriteFailed {
            path: modulemap_path,
            source,
        })
    }
}

struct XcframeworkLibraryInput {
    library_path: PathBuf,
    headers_path: PathBuf,
}

impl XcframeworkLibraryInput {
    fn library_path(&self) -> &Path {
        &self.library_path
    }

    fn headers_path(&self) -> &Path {
        &self.headers_path
    }
}

#[derive(Template)]
#[template(path = "AppleLibrary.modulemap", escape = "none")]
struct AppleLibraryModulemapTemplate<'a> {
    module_name: &'a str,
    header_path: &'a str,
}

fn render_library_modulemap(module_name: &str, header_path: &str) -> Result<String> {
    AppleLibraryModulemapTemplate {
        module_name,
        header_path,
    }
    .render()
    .map_err(|source| CliError::CommandFailed {
        command: format!("render Apple library modulemap template: {source}"),
        status: None,
    })
}

fn remove_directory_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_dir_all(path).map_err(|source| CliError::CreateDirectoryFailed {
        path: path.to_path_buf(),
        source,
    })
}

fn copy_directory_contents(from: &Path, to: &Path) -> Result<()> {
    walkdir::WalkDir::new(from)
        .into_iter()
        .try_for_each(|entry| {
            let entry = entry.map_err(|error| {
                let path = error
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| from.to_path_buf());
                let source = error
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other("directory walk failed"));

                CliError::ReadFailed { path, source }
            })?;

            if !entry.file_type().is_file() {
                return Ok(());
            }

            let relative =
                entry
                    .path()
                    .strip_prefix(from)
                    .map_err(|source| CliError::ReadFailed {
                        path: entry.path().to_path_buf(),
                        source: std::io::Error::other(source.to_string()),
                    })?;
            let dest = to.join(relative);

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }

            fs::copy(entry.path(), &dest).map_err(|source| CliError::CopyFailed {
                from: entry.path().to_path_buf(),
                to: dest,
                source,
            })?;

            Ok(())
        })
}

fn create_zip(source_dir: &Path, zip_path: &Path) -> Result<()> {
    let file = fs::File::create(zip_path).map_err(|source| CliError::WriteFailed {
        path: zip_path.to_path_buf(),
        source,
    })?;

    let mut zip_writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .try_for_each(|entry| {
            let relative = entry
                .path()
                .strip_prefix(source_dir.parent().unwrap())
                .unwrap();
            let path_string = relative.to_string_lossy().to_string();

            if entry.file_type().is_symlink() {
                let target =
                    fs::read_link(entry.path()).map_err(|source| CliError::ReadFailed {
                        path: entry.path().to_path_buf(),
                        source,
                    })?;

                zip_writer
                    .add_symlink(path_string, target.to_string_lossy().to_string(), options)
                    .map_err(|_| PackError::ZipFailed {
                        source: std::io::Error::other("zip symlink failed"),
                    })?;
            } else if entry.file_type().is_dir() {
                zip_writer
                    .add_directory(path_string, options)
                    .map_err(|_| PackError::ZipFailed {
                        source: std::io::Error::other("zip dir failed"),
                    })?;
            } else {
                zip_writer
                    .start_file(path_string, options)
                    .map_err(|_| PackError::ZipFailed {
                        source: std::io::Error::other("zip start failed"),
                    })?;

                let content = fs::read(entry.path()).map_err(|source| CliError::ReadFailed {
                    path: entry.path().to_path_buf(),
                    source,
                })?;

                std::io::Write::write_all(&mut zip_writer, &content)
                    .map_err(|source| PackError::ZipFailed { source })?;
            }

            Ok::<_, CliError>(())
        })?;

    zip_writer.finish().map_err(|_| PackError::ZipFailed {
        source: std::io::Error::other("zip finish failed"),
    })?;

    Ok(())
}

pub(crate) fn compute_checksum(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let content = fs::read(path).map_err(|source| CliError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    let hash = Sha256::digest(&content);
    Ok(hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        AppleLibrarySlice, AppleLibrarySliceKind, AppleNames, StaticLibrarySlicePlan,
        XcframeworkPlan,
    };
    use crate::config::{Config, PackageConfig, TargetsConfig};

    struct TemporaryDirectory {
        path: PathBuf,
    }

    impl TemporaryDirectory {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{prefix}-{unique}"));
            fs::create_dir_all(&path).expect("create temporary directory");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn config() -> Config {
        Config {
            experimental: Vec::new(),
            cargo: Default::default(),
            package: PackageConfig {
                name: "demo".to_string(),
                crate_name: None,
                version: None,
                description: None,
                license: None,
                repository: None,
            },
            targets: TargetsConfig::default(),
        }
    }

    #[test]
    fn stages_xcframework_inputs_under_scratch_directory() {
        let temporary_directory = TemporaryDirectory::new("boltffi-xcframework-plan");
        let output_dir = temporary_directory.path().join("dist").join("apple");
        let scratch_dir = temporary_directory
            .path()
            .join("target")
            .join("boltffi")
            .join("pack")
            .join("apple")
            .join("xcframework");
        let headers_path = temporary_directory.path().join("headers");
        let library_path = temporary_directory.path().join("libdemo.a");
        let names = AppleNames::from_config(&config());

        let plan = XcframeworkPlan::new(
            &output_dir,
            &scratch_dir,
            &names,
            headers_path,
            vec![AppleLibrarySlice::resolved(
                AppleLibrarySliceKind::IosSimulator,
                library_path,
            )],
        );

        assert_eq!(plan.library_staging_dir, scratch_dir.join("libraries"));
        assert!(
            plan.library_plans
                .iter()
                .all(|library_plan| library_plan.headers_path.starts_with(&scratch_dir))
        );
        assert_eq!(
            plan.xcframework_path,
            output_dir.join(format!("{}.xcframework", names.xcframework_name()))
        );
    }

    #[test]
    fn prepare_output_removes_legacy_public_staging_directories() {
        let temporary_directory = TemporaryDirectory::new("boltffi-xcframework-cleanup");
        let output_dir = temporary_directory.path().join("dist").join("apple");
        let scratch_dir = temporary_directory
            .path()
            .join("target")
            .join("apple-scratch");
        let names = AppleNames::from_config(&config());

        [
            output_dir.join("headers_staging"),
            output_dir.join("framework_staging"),
            output_dir.join("ios-device-fat"),
            output_dir.join("ios-simulator-fat"),
            output_dir.join("macos-fat"),
            output_dir.join(format!("{}.xcframework", names.xcframework_name())),
            scratch_dir.join("frameworks"),
            scratch_dir.join("libraries"),
        ]
        .into_iter()
        .try_for_each(fs::create_dir_all)
        .expect("create stale directories");

        let plan = XcframeworkPlan::new(
            &output_dir,
            &scratch_dir,
            &names,
            output_dir.join("headers"),
            Vec::new(),
        );
        plan.prepare_output().expect("prepare xcframework output");

        [
            temporary_directory
                .path()
                .join("dist")
                .join("apple")
                .join("headers_staging"),
            temporary_directory
                .path()
                .join("dist")
                .join("apple")
                .join("framework_staging"),
            temporary_directory
                .path()
                .join("dist")
                .join("apple")
                .join("ios-device-fat"),
            temporary_directory
                .path()
                .join("dist")
                .join("apple")
                .join("ios-simulator-fat"),
            temporary_directory
                .path()
                .join("dist")
                .join("apple")
                .join("macos-fat"),
            temporary_directory
                .path()
                .join("dist")
                .join("apple")
                .join(format!("{}.xcframework", names.xcframework_name())),
        ]
        .into_iter()
        .for_each(|path| assert!(!path.exists()));
        assert!(scratch_dir.join("libraries").is_dir());
    }

    #[test]
    fn creates_static_library_headers() {
        let temporary_directory = TemporaryDirectory::new("boltffi-static-library-headers");
        let headers_path = temporary_directory.path().join("headers");
        let private_headers_path = headers_path.join("private");
        let library_path = temporary_directory.path().join("libdemo.a");
        let slice_headers_path = temporary_directory.path().join("slice").join("Headers");

        fs::create_dir_all(&private_headers_path).expect("create private headers");
        fs::write(headers_path.join("demo.h"), "").expect("write public header");
        fs::write(private_headers_path.join("detail.h"), "").expect("write private header");
        fs::write(&library_path, "archive").expect("write static library");

        let input = StaticLibrarySlicePlan::new(
            slice_headers_path.clone(),
            "DemoFFI".to_string(),
            "demo".to_string(),
            headers_path.clone(),
            library_path.clone(),
        )
        .execute()
        .expect("create static library headers");

        assert_eq!(input.library_path(), library_path);
        assert_eq!(input.headers_path(), slice_headers_path);
        assert_eq!(
            fs::read_to_string(slice_headers_path.join("module.modulemap"))
                .expect("read library module map"),
            r#"module DemoFFI {
    header "demo/demo.h"
    export *
}"#
        );
        assert!(
            slice_headers_path
                .join("demo")
                .join("private")
                .join("detail.h")
                .is_file()
        );
    }
}
