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
            self.config.apple_deployment_target(),
            self.headers_dir.clone(),
            library_slices,
        );

        plan.prepare_output()?;
        let framework_paths = plan.create_static_frameworks()?;
        self.run_create_xcframework(&plan.xcframework_path, &framework_paths)?;

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
        framework_paths: &[PathBuf],
    ) -> Result<()> {
        let mut xcodebuild_cmd = Command::new("xcodebuild");
        xcodebuild_cmd.arg("-create-xcframework");

        framework_paths.iter().for_each(|framework_path| {
            xcodebuild_cmd.arg("-framework").arg(framework_path);
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

    fn framework_layout(self) -> FrameworkLayout {
        match self {
            Self::IosDevice | Self::IosSimulator => FrameworkLayout::Shallow,
            Self::MacOs => FrameworkLayout::Versioned,
        }
    }

    fn minimum_os_version(self, deployment_target: &str) -> Option<String> {
        match self {
            Self::IosDevice | Self::IosSimulator => Some(deployment_target.to_owned()),
            Self::MacOs => None,
        }
    }
}

/// Layout of a `.framework` bundle.
///
/// macOS is the only Apple platform that requires the versioned bundle layout
/// (`Versions/A` plus `Current` and top-level symlinks); every other platform
/// uses the shallow layout with resources at the bundle root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameworkLayout {
    Shallow,
    Versioned,
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
    framework_staging_dir: PathBuf,
    framework_plans: Vec<StaticFrameworkBundlePlan>,
}

impl XcframeworkPlan {
    fn new(
        output_dir: &Path,
        scratch_dir: &Path,
        names: &AppleNames,
        deployment_target: &str,
        headers_dir: PathBuf,
        library_slices: Vec<AppleLibrarySlice>,
    ) -> Self {
        let xcframework_path = output_dir.join(format!("{}.xcframework", names.xcframework_name()));
        let framework_staging_dir = scratch_dir.join("frameworks");
        let framework_name = names.ffi_module_name().to_string();
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

        let framework_plans = library_slices
            .into_iter()
            .map(|library_slice| {
                let framework_path = framework_staging_dir
                    .join(library_slice.staging_directory_name())
                    .join(format!("{framework_name}.framework"));

                StaticFrameworkBundlePlan::new(
                    framework_path,
                    framework_name.clone(),
                    library_name.clone(),
                    headers_dir.clone(),
                    library_slice.library_path,
                    library_slice.kind.framework_layout(),
                    library_slice.kind.minimum_os_version(deployment_target),
                )
            })
            .collect();

        Self {
            xcframework_path,
            legacy_output_staging_dirs,
            framework_staging_dir,
            framework_plans,
        }
    }

    fn prepare_output(&self) -> Result<()> {
        remove_directory_if_exists(&self.xcframework_path)?;
        self.legacy_output_staging_dirs
            .iter()
            .try_for_each(|path| remove_directory_if_exists(path))?;
        remove_directory_if_exists(&self.framework_staging_dir)?;

        fs::create_dir_all(&self.framework_staging_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: self.framework_staging_dir.clone(),
                source,
            }
        })
    }

    fn create_static_frameworks(&self) -> Result<Vec<PathBuf>> {
        self.framework_plans
            .iter()
            .map(StaticFrameworkBundlePlan::execute)
            .collect()
    }
}

struct StaticFrameworkBundlePlan {
    framework_path: PathBuf,
    framework_name: String,
    library_name: String,
    headers_dir: PathBuf,
    library_path: PathBuf,
    public_header_path: String,
    layout: FrameworkLayout,
    minimum_os_version: Option<String>,
}

impl StaticFrameworkBundlePlan {
    fn new(
        framework_path: PathBuf,
        framework_name: String,
        library_name: String,
        headers_dir: PathBuf,
        library_path: PathBuf,
        layout: FrameworkLayout,
        minimum_os_version: Option<String>,
    ) -> Self {
        let public_header_path = format!("{}/{}.h", library_name, library_name);

        Self {
            framework_path,
            framework_name,
            library_name,
            headers_dir,
            library_path,
            public_header_path,
            layout,
            minimum_os_version,
        }
    }

    fn execute(&self) -> Result<PathBuf> {
        remove_directory_if_exists(&self.framework_path)?;

        let namespaced_headers_path = self.namespaced_headers_path();
        let modules_path = self.modules_path();

        fs::create_dir_all(&namespaced_headers_path).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: namespaced_headers_path.clone(),
                source,
            }
        })?;
        fs::create_dir_all(&modules_path).map_err(|source| CliError::CreateDirectoryFailed {
            path: modules_path.clone(),
            source,
        })?;

        fs::copy(&self.library_path, self.framework_binary_path()).map_err(|source| {
            CliError::CopyFailed {
                from: self.library_path.clone(),
                to: self.framework_binary_path(),
                source,
            }
        })?;

        copy_directory_contents(&self.headers_dir, &namespaced_headers_path)?;
        self.write_modulemap()?;
        self.write_info_plist()?;

        if self.layout == FrameworkLayout::Versioned {
            self.create_version_symlinks()?;
        }

        Ok(self.framework_path.clone())
    }

    /// Directory that holds the bundle's contents (binary, Headers, Modules,
    /// Info.plist). For shallow bundles this is the framework root; for
    /// versioned bundles it is `Versions/A`.
    fn contents_dir(&self) -> PathBuf {
        match self.layout {
            FrameworkLayout::Shallow => self.framework_path.clone(),
            FrameworkLayout::Versioned => self.framework_path.join("Versions").join("A"),
        }
    }

    fn namespaced_headers_path(&self) -> PathBuf {
        self.contents_dir().join("Headers").join(&self.library_name)
    }

    fn modules_path(&self) -> PathBuf {
        self.contents_dir().join("Modules")
    }

    fn framework_binary_path(&self) -> PathBuf {
        self.contents_dir().join(&self.framework_name)
    }

    fn info_plist_path(&self) -> PathBuf {
        match self.layout {
            // Shallow bundles keep Info.plist at the bundle root.
            FrameworkLayout::Shallow => self.framework_path.join("Info.plist"),
            // Versioned bundles place it under Resources, which macOS requires.
            FrameworkLayout::Versioned => self.contents_dir().join("Resources").join("Info.plist"),
        }
    }

    fn write_modulemap(&self) -> Result<()> {
        let modulemap_content =
            render_framework_modulemap(&self.framework_name, &self.public_header_path)?;
        let modulemap_path = self.modules_path().join("module.modulemap");

        fs::write(&modulemap_path, modulemap_content).map_err(|source| CliError::WriteFailed {
            path: modulemap_path,
            source,
        })
    }

    fn write_info_plist(&self) -> Result<()> {
        let info_plist_content =
            render_framework_info_plist(&self.framework_name, self.minimum_os_version.as_deref())?;
        let info_plist_path = self.info_plist_path();

        if let Some(parent) = info_plist_path.parent() {
            fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        fs::write(&info_plist_path, info_plist_content).map_err(|source| CliError::WriteFailed {
            path: info_plist_path,
            source,
        })
    }

    /// Create the symlinks that turn a `Versions/A` tree into a valid versioned
    /// macOS framework bundle:
    ///
    /// ```text
    /// Versions/Current -> A
    /// {framework_name} -> Versions/Current/{framework_name}
    /// Headers          -> Versions/Current/Headers
    /// Modules          -> Versions/Current/Modules
    /// Resources        -> Versions/Current/Resources
    /// ```
    fn create_version_symlinks(&self) -> Result<()> {
        create_relative_symlink(
            Path::new("A"),
            &self.framework_path.join("Versions").join("Current"),
        )?;

        for entry in [
            self.framework_name.as_str(),
            "Headers",
            "Modules",
            "Resources",
        ] {
            let target = Path::new("Versions").join("Current").join(entry);
            create_relative_symlink(&target, &self.framework_path.join(entry))?;
        }

        Ok(())
    }
}

#[derive(Template)]
#[template(path = "AppleFrameworkInfo.plist.xml", escape = "html")]
struct AppleFrameworkInfoPlistTemplate<'a> {
    framework_name: &'a str,
    bundle_identifier: &'a str,
    minimum_os_version: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "AppleFramework.modulemap", escape = "none")]
struct AppleFrameworkModulemapTemplate<'a> {
    module_name: &'a str,
    header_path: &'a str,
}

fn render_framework_info_plist(
    framework_name: &str,
    minimum_os_version: Option<&str>,
) -> Result<String> {
    let bundle_identifier = framework_bundle_identifier(framework_name);

    AppleFrameworkInfoPlistTemplate {
        framework_name,
        bundle_identifier: &bundle_identifier,
        minimum_os_version,
    }
    .render()
    .map_err(|source| CliError::CommandFailed {
        command: format!("render Apple framework Info.plist template: {source}"),
        status: None,
    })
}

fn framework_bundle_identifier(framework_name: &str) -> String {
    let suffix = framework_name
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if character == '-' || character == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();

    if suffix.is_empty() {
        "dev.boltffi.ffi".to_string()
    } else {
        format!("dev.boltffi.{suffix}")
    }
}

fn render_framework_modulemap(module_name: &str, header_path: &str) -> Result<String> {
    AppleFrameworkModulemapTemplate {
        module_name,
        header_path,
    }
    .render()
    .map_err(|source| CliError::CommandFailed {
        command: format!("render Apple framework modulemap template: {source}"),
        status: None,
    })
}

/// Create a symlink at `link` pointing at the relative `target`.
fn create_relative_symlink(target: &Path, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).map_err(|source| CliError::WriteFailed {
            path: link.to_path_buf(),
            source,
        })
    }

    #[cfg(not(unix))]
    {
        let _ = target;
        Err(CliError::WriteFailed {
            path: link.to_path_buf(),
            source: std::io::Error::other(
                "creating macOS versioned framework bundles requires symlink support",
            ),
        })
    }
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

            // The versioned macOS framework layout stores its top-level entries
            // (binary, Headers, Modules, Resources, Versions/Current) as
            // symlinks. walkdir does not follow them, so preserve them as
            // symlink entries instead of trying to read them as files.
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
        AppleLibrarySlice, AppleLibrarySliceKind, AppleNames, FrameworkLayout,
        StaticFrameworkBundlePlan, XcframeworkPlan, create_zip,
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
            "16.0",
            headers_path,
            vec![AppleLibrarySlice::resolved(
                AppleLibrarySliceKind::IosSimulator,
                library_path,
            )],
        );

        assert_eq!(plan.framework_staging_dir, scratch_dir.join("frameworks"));
        assert!(
            plan.framework_plans
                .iter()
                .all(|framework_plan| framework_plan.framework_path.starts_with(&scratch_dir))
        );
        assert!(
            plan.framework_plans
                .iter()
                .all(|framework_plan| framework_plan.minimum_os_version.as_deref() == Some("16.0"))
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
        ]
        .into_iter()
        .try_for_each(fs::create_dir_all)
        .expect("create stale directories");

        let plan = XcframeworkPlan::new(
            &output_dir,
            &scratch_dir,
            &names,
            "16.0",
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
        assert!(scratch_dir.join("frameworks").is_dir());
    }

    #[test]
    fn creates_static_framework_bundle() {
        let temporary_directory = TemporaryDirectory::new("boltffi-static-framework");
        let headers_path = temporary_directory.path().join("headers");
        let private_headers_path = headers_path.join("private");
        let library_path = temporary_directory.path().join("libdemo.a");
        let framework_path = temporary_directory.path().join("DemoFFI.framework");

        fs::create_dir_all(&private_headers_path).expect("create private headers");
        fs::write(headers_path.join("demo.h"), "").expect("write public header");
        fs::write(private_headers_path.join("detail.h"), "").expect("write private header");
        fs::write(&library_path, "archive").expect("write static library");

        let created_framework_path = StaticFrameworkBundlePlan::new(
            framework_path.clone(),
            "DemoFFI".to_string(),
            "demo".to_string(),
            headers_path.clone(),
            library_path.clone(),
            FrameworkLayout::Shallow,
            Some("16.0".to_string()),
        )
        .execute()
        .expect("create static framework bundle");

        assert_eq!(created_framework_path, framework_path);
        assert_eq!(
            fs::read_to_string(framework_path.join("DemoFFI")).expect("read framework binary"),
            "archive"
        );
        assert!(
            framework_path
                .join("Headers")
                .join("demo")
                .join("demo.h")
                .is_file()
        );
        assert_eq!(
            fs::read_to_string(framework_path.join("Modules").join("module.modulemap"))
                .expect("read framework module map"),
            r#"framework module DemoFFI {
    header "demo/demo.h"
    export *
}"#
        );
        assert!(
            framework_path
                .join("Headers")
                .join("demo")
                .join("private")
                .join("detail.h")
                .is_file()
        );
        assert!(framework_path.join("Info.plist").is_file());
        assert!(
            fs::read_to_string(framework_path.join("Info.plist"))
                .expect("read framework plist")
                .contains("<key>MinimumOSVersion</key>\n    <string>16.0</string>")
        );
        assert!(
            !framework_path
                .join("Headers")
                .join("module.modulemap")
                .exists()
        );
    }

    #[test]
    fn macos_slice_uses_versioned_layout() {
        assert_eq!(
            AppleLibrarySliceKind::MacOs.framework_layout(),
            FrameworkLayout::Versioned
        );
        assert_eq!(
            AppleLibrarySliceKind::IosDevice.framework_layout(),
            FrameworkLayout::Shallow
        );
        assert_eq!(
            AppleLibrarySliceKind::IosSimulator.framework_layout(),
            FrameworkLayout::Shallow
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_versioned_framework_bundle_for_macos() {
        let temporary_directory = TemporaryDirectory::new("boltffi-versioned-framework");
        let headers_path = temporary_directory.path().join("headers");
        let library_path = temporary_directory.path().join("libdemo.a");
        let framework_path = temporary_directory.path().join("DemoFFI.framework");

        fs::create_dir_all(&headers_path).expect("create headers");
        fs::write(headers_path.join("demo.h"), "").expect("write public header");
        fs::write(&library_path, "archive").expect("write static library");

        StaticFrameworkBundlePlan::new(
            framework_path.clone(),
            "DemoFFI".to_string(),
            "demo".to_string(),
            headers_path.clone(),
            library_path.clone(),
            FrameworkLayout::Versioned,
            None,
        )
        .execute()
        .expect("create versioned framework bundle");

        let versions_a = framework_path.join("Versions").join("A");

        // Real contents live under Versions/A.
        assert_eq!(
            fs::read_to_string(versions_a.join("DemoFFI")).expect("read framework binary"),
            "archive"
        );
        assert!(
            versions_a
                .join("Headers")
                .join("demo")
                .join("demo.h")
                .is_file()
        );
        assert!(
            versions_a
                .join("Modules")
                .join("module.modulemap")
                .is_file()
        );
        // macOS requires Info.plist under Resources, not the bundle root.
        assert!(versions_a.join("Resources").join("Info.plist").is_file());
        assert!(
            !fs::read_to_string(versions_a.join("Resources").join("Info.plist"))
                .expect("read framework plist")
                .contains("MinimumOSVersion")
        );
        assert!(!framework_path.join("Info.plist").exists());

        // Versions/Current -> A
        assert_eq!(
            fs::read_link(framework_path.join("Versions").join("Current"))
                .expect("read Current symlink"),
            Path::new("A")
        );

        // Top-level symlinks point through Versions/Current.
        for entry in ["DemoFFI", "Headers", "Modules", "Resources"] {
            let link = framework_path.join(entry);
            assert_eq!(
                fs::read_link(&link).unwrap_or_else(|_| panic!("read {entry} symlink")),
                Path::new("Versions").join("Current").join(entry)
            );
            // Symlink resolves to a real file/directory.
            assert!(link.exists(), "{entry} symlink should resolve");
        }
    }

    #[cfg(unix)]
    #[test]
    fn zips_versioned_framework_symlinks_as_symlinks() {
        let temporary_directory = TemporaryDirectory::new("boltffi-zip-versioned");
        let headers_path = temporary_directory.path().join("headers");
        let library_path = temporary_directory.path().join("libdemo.a");
        let framework_path = temporary_directory.path().join("DemoFFI.framework");

        fs::create_dir_all(&headers_path).expect("create headers");
        fs::write(headers_path.join("demo.h"), "").expect("write public header");
        fs::write(&library_path, "archive").expect("write static library");

        // The versioned macOS layout produces top-level symlinks (Resources,
        // Headers, Modules, the binary, Versions/Current). Zipping used to fail
        // by trying to read these symlinks as files.
        StaticFrameworkBundlePlan::new(
            framework_path.clone(),
            "DemoFFI".to_string(),
            "demo".to_string(),
            headers_path,
            library_path,
            FrameworkLayout::Versioned,
            None,
        )
        .execute()
        .expect("create versioned framework bundle");

        let zip_path = temporary_directory.path().join("DemoFFI.framework.zip");
        create_zip(&framework_path, &zip_path).expect("zip versioned framework bundle");

        let archive_file = fs::File::open(&zip_path).expect("open zip archive");
        let mut archive = zip::ZipArchive::new(archive_file).expect("read zip archive");

        let resources_entry = archive
            .by_name("DemoFFI.framework/Resources")
            .expect("zip should contain the Resources symlink entry");
        assert!(
            resources_entry.is_symlink(),
            "Resources should be stored as a symlink, not a regular file"
        );
        drop(resources_entry);

        // The real contents under Versions/A are still stored as files.
        assert!(
            archive
                .by_name("DemoFFI.framework/Versions/A/DemoFFI")
                .is_ok(),
            "versioned binary should be present in the archive"
        );
    }
}
