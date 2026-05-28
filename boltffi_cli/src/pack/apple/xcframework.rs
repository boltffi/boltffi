use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::pack::PackError;
use crate::target::{BuiltLibrary, Platform};

pub struct XcframeworkBuilder<'a> {
    config: &'a Config,
    libraries: Vec<BuiltLibrary>,
    headers_dir: PathBuf,
    output_dir: PathBuf,
}

pub struct XcframeworkOutput {
    pub xcframework_path: PathBuf,
    pub zip_path: Option<PathBuf>,
    pub checksum: Option<String>,
}

impl<'a> XcframeworkBuilder<'a> {
    pub fn new(config: &'a Config, libraries: Vec<BuiltLibrary>, headers_dir: PathBuf) -> Self {
        Self {
            config,
            libraries,
            headers_dir,
            output_dir: config.apple_xcframework_output(),
        }
    }

    pub fn build(self) -> Result<XcframeworkOutput> {
        fs::create_dir_all(&self.output_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: self.output_dir.clone(),
            source,
        })?;

        let device_libs = self.filter_device_libraries();
        let simulator_libs = self.filter_simulator_libraries();
        let macos_libs = self.filter_macos_libraries();

        let fat_sim_lib = self.create_fat_library(&simulator_libs, "ios-simulator-fat")?;
        let fat_macos_lib = self.create_fat_library(&macos_libs, "macos-fat")?;

        let xcframework_path =
            self.create_xcframework(&device_libs, fat_sim_lib.as_ref(), fat_macos_lib.as_ref())?;

        Ok(XcframeworkOutput {
            xcframework_path,
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

    fn filter_device_libraries(&self) -> Vec<&BuiltLibrary> {
        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::Ios)
            .collect()
    }

    fn filter_simulator_libraries(&self) -> Vec<&BuiltLibrary> {
        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::IosSimulator)
            .collect()
    }

    fn filter_macos_libraries(&self) -> Vec<&BuiltLibrary> {
        if !self.config.apple_include_macos() {
            return Vec::new();
        }

        self.libraries
            .iter()
            .filter(|lib| lib.target.platform() == Platform::MacOs)
            .collect()
    }

    fn create_fat_library(
        &self,
        libs: &[&BuiltLibrary],
        output_dir_name: &str,
    ) -> Result<Option<PathBuf>> {
        if libs.is_empty() {
            return Ok(None);
        }

        if libs.len() == 1 {
            return Ok(Some(libs[0].path.clone()));
        }

        let fat_dir = self.output_dir.join(output_dir_name);
        fs::create_dir_all(&fat_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: fat_dir.clone(),
            source,
        })?;

        let lib_name = self.config.library_name();
        let fat_lib_path = fat_dir.join(format!("lib{}.a", lib_name));

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

    fn create_xcframework(
        &self,
        device_libs: &[&BuiltLibrary],
        fat_sim_lib: Option<&PathBuf>,
        fat_macos_lib: Option<&PathBuf>,
    ) -> Result<PathBuf> {
        let xcframework_name = self.config.xcframework_name();
        let xcframework_path = self
            .output_dir
            .join(format!("{}.xcframework", xcframework_name));

        if xcframework_path.exists() {
            fs::remove_dir_all(&xcframework_path).map_err(|source| {
                CliError::CreateDirectoryFailed {
                    path: xcframework_path.clone(),
                    source,
                }
            })?;
        }

        let headers_staging = self.prepare_headers()?;

        let mut xcodebuild_cmd = Command::new("xcodebuild");
        xcodebuild_cmd.arg("-create-xcframework");

        device_libs.iter().for_each(|lib| {
            xcodebuild_cmd
                .arg("-library")
                .arg(&lib.path)
                .arg("-headers")
                .arg(&headers_staging);
        });

        if let Some(sim_lib) = fat_sim_lib {
            xcodebuild_cmd
                .arg("-library")
                .arg(sim_lib)
                .arg("-headers")
                .arg(&headers_staging);
        }

        if let Some(macos_lib) = fat_macos_lib {
            xcodebuild_cmd
                .arg("-library")
                .arg(macos_lib)
                .arg("-headers")
                .arg(&headers_staging);
        }

        xcodebuild_cmd.arg("-output").arg(&xcframework_path);
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

        HeaderNamespace::new(self.config.library_name(), self.config.xcframework_name())
            .apply_to_xcframework(&xcframework_path)?;

        Ok(xcframework_path)
    }

    fn prepare_headers(&self) -> Result<PathBuf> {
        let headers_staging = self.output_dir.join("headers_staging");

        if headers_staging.exists() {
            fs::remove_dir_all(&headers_staging).map_err(|source| {
                CliError::CreateDirectoryFailed {
                    path: headers_staging.clone(),
                    source,
                }
            })?;
        }

        fs::create_dir_all(&headers_staging).map_err(|source| CliError::CreateDirectoryFailed {
            path: headers_staging.clone(),
            source,
        })?;

        copy_directory_contents(&self.headers_dir, &headers_staging)?;

        let modulemap_content = generate_modulemap(
            &self.config.xcframework_name(),
            &format!("{}.h", self.config.library_name()),
        );
        let modulemap_path = headers_staging.join("module.modulemap");

        fs::write(&modulemap_path, modulemap_content).map_err(|source| CliError::WriteFailed {
            path: modulemap_path,
            source,
        })?;

        Ok(headers_staging)
    }
}

struct HeaderNamespace {
    directory_name: String,
    module_name: String,
}

impl HeaderNamespace {
    fn new(library_name: impl Into<String>, module_name: impl Into<String>) -> Self {
        Self {
            directory_name: library_name.into(),
            module_name: module_name.into(),
        }
    }

    fn apply_to_xcframework(&self, xcframework_path: &Path) -> Result<()> {
        fs::read_dir(xcframework_path)
            .map_err(|source| CliError::ReadFailed {
                path: xcframework_path.to_path_buf(),
                source,
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| CliError::ReadFailed {
                path: xcframework_path.to_path_buf(),
                source,
            })?
            .into_iter()
            .map(|entry| entry.path().join("Headers"))
            .filter(|headers_path| headers_path.is_dir())
            .try_for_each(|headers_path| self.apply_to_headers_dir(&headers_path))
    }

    fn apply_to_headers_dir(&self, headers_path: &Path) -> Result<()> {
        let namespace_path = headers_path.join(&self.directory_name);
        let entries = fs::read_dir(headers_path)
            .map_err(|source| CliError::ReadFailed {
                path: headers_path.to_path_buf(),
                source,
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| CliError::ReadFailed {
                path: headers_path.to_path_buf(),
                source,
            })?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path != &namespace_path)
            .filter(|path| {
                path.file_name()
                    .is_none_or(|file_name| file_name != "module.modulemap")
            })
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return Ok(());
        }

        fs::create_dir_all(&namespace_path).map_err(|source| CliError::CreateDirectoryFailed {
            path: namespace_path.clone(),
            source,
        })?;

        entries
            .into_iter()
            .try_for_each(|source_path| self.move_entry(source_path, &namespace_path))?;

        self.write_root_modulemap(headers_path)
    }

    fn move_entry(&self, source_path: PathBuf, namespace_path: &Path) -> Result<()> {
        let file_name = source_path
            .file_name()
            .map(|file_name| file_name.to_owned())
            .ok_or_else(|| CliError::FileNotFound(source_path.clone()))?;
        let target_path = namespace_path.join(file_name);

        fs::rename(&source_path, &target_path).map_err(|source| CliError::WriteFailed {
            path: target_path,
            source,
        })
    }

    fn write_root_modulemap(&self, headers_path: &Path) -> Result<()> {
        let modulemap_path = headers_path.join("module.modulemap");
        let namespaced_header_path = format!("{}/{}.h", self.directory_name, self.directory_name);
        let modulemap_content = generate_modulemap(&self.module_name, &namespaced_header_path);

        fs::write(&modulemap_path, modulemap_content).map_err(|source| CliError::WriteFailed {
            path: modulemap_path,
            source,
        })
    }
}

fn generate_modulemap(module_name: &str, header_path: &str) -> String {
    format!(
        r#"module {}FFI {{
    header "{}"
    export *
}}
"#,
        module_name, header_path
    )
}

fn copy_directory_contents(from: &Path, to: &Path) -> Result<()> {
    walkdir::WalkDir::new(from)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .try_for_each(|entry| {
            let relative = entry.path().strip_prefix(from).unwrap();
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

            if entry.file_type().is_dir() {
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

    use super::{HeaderNamespace, generate_modulemap};

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

    #[test]
    fn namespaces_slice_headers() {
        let temporary_directory = TemporaryDirectory::new("boltffi-xcframework-headers");
        let xcframework_path = temporary_directory.path().join("Demo.xcframework");
        let headers_path = xcframework_path.join("ios-arm64").join("Headers");
        let private_headers_path = headers_path.join("private");
        fs::create_dir_all(&private_headers_path).expect("create private headers");
        fs::write(headers_path.join("demo.h"), "").expect("write public header");
        fs::write(
            headers_path.join("module.modulemap"),
            generate_modulemap("Demo", "demo.h"),
        )
        .expect("write module map");
        fs::write(private_headers_path.join("detail.h"), "").expect("write private header");

        HeaderNamespace::new("demo", "Demo")
            .apply_to_xcframework(&xcframework_path)
            .expect("namespace headers");

        assert!(headers_path.join("demo").join("demo.h").is_file());
        assert!(!headers_path.join("demo").join("module.modulemap").exists());
        assert_eq!(
            fs::read_to_string(headers_path.join("module.modulemap"))
                .expect("read root module map"),
            r#"module DemoFFI {
    header "demo/demo.h"
    export *
}
"#
        );
        assert!(
            headers_path
                .join("demo")
                .join("private")
                .join("detail.h")
                .is_file()
        );
        assert!(!headers_path.join("demo.h").exists());
        assert!(!headers_path.join("private").exists());
    }

    #[test]
    fn keeps_namespaced_headers_stable() {
        let temporary_directory = TemporaryDirectory::new("boltffi-xcframework-namespaced-headers");
        let xcframework_path = temporary_directory.path().join("Demo.xcframework");
        let headers_path = xcframework_path.join("ios-arm64").join("Headers");
        let namespaced_headers_path = headers_path.join("demo");

        fs::create_dir_all(&namespaced_headers_path).expect("create namespaced headers");
        fs::write(namespaced_headers_path.join("demo.h"), "").expect("write public header");
        fs::write(namespaced_headers_path.join("module.modulemap"), "").expect("write module map");

        HeaderNamespace::new("demo", "Demo")
            .apply_to_xcframework(&xcframework_path)
            .expect("namespace headers");

        assert!(namespaced_headers_path.join("demo.h").is_file());
        assert!(namespaced_headers_path.join("module.modulemap").is_file());
        assert_eq!(
            fs::read_dir(&headers_path)
                .expect("read headers directory")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("read header entry")
                .len(),
            1
        );
    }
}
