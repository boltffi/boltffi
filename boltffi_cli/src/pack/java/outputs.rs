use std::path::Path;

use crate::cli::{CliError, Result};
use crate::target::JavaHostTarget;

use super::link::JvmNativePackageLayout;

impl JvmNativePackageLayout {
    pub fn remove_stale_host_artifacts(
        &self,
        requested_host_targets: &[JavaHostTarget],
        rust_artifact_name: &str,
    ) -> Result<()> {
        [
            JavaHostTarget::DarwinArm64,
            JavaHostTarget::DarwinX86_64,
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::LinuxAarch64,
            JavaHostTarget::WindowsX86_64,
        ]
        .into_iter()
        .filter(|host_target| !requested_host_targets.contains(host_target))
        .try_for_each(|host_target| {
            let host_directory = self.native_output_root.join(host_target.canonical_name());
            match std::fs::symlink_metadata(&host_directory) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(source) => {
                    return Err(CliError::ReadFailed {
                        path: host_directory,
                        source,
                    });
                }
            }

            [self.jni_library_name.as_str(), rust_artifact_name]
                .into_iter()
                .try_for_each(|library_name| {
                    let library_path =
                        host_directory.join(host_target.shared_library_filename(library_name));
                    remove_file_if_exists(&library_path)?;
                    match host_target {
                        JavaHostTarget::WindowsX86_64 => {
                            remove_file_if_exists(&library_path.with_extension("pdb"))
                        }
                        JavaHostTarget::DarwinArm64 | JavaHostTarget::DarwinX86_64 => {
                            remove_directory_if_exists(&library_path.with_added_extension("dSYM"))
                        }
                        _ => Ok(()),
                    }
                })
        })
    }
}

pub(crate) fn remove_file_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CliError::WriteFailed {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(crate) fn remove_stale_flat_jvm_outputs_if_current_host_unrequested(
    java_output: &Path,
    current_host: Option<JavaHostTarget>,
    requested_host_targets: &[JavaHostTarget],
    artifact_name: &str,
) -> Result<()> {
    let Some(current_host) = current_host else {
        return Ok(());
    };

    if requested_host_targets.contains(&current_host) {
        return Ok(());
    }

    remove_file_if_exists(&java_output.join(current_host.jni_library_filename(artifact_name)))?;
    remove_file_if_exists(&java_output.join(current_host.shared_library_filename(artifact_name)))?;
    Ok(())
}

pub(crate) fn remove_stale_requested_jvm_shared_library_copies_after_success(
    java_output: &Path,
    packaged_outputs: &[super::link::JvmPackagedNativeOutput],
    artifact_name: &str,
) -> Result<()> {
    let current_host = JavaHostTarget::current();

    for packaged_output in packaged_outputs {
        if packaged_output.has_shared_library_copy {
            continue;
        }

        let stale_shared_library_name = packaged_output
            .host_target
            .shared_library_filename(artifact_name);
        let structured_copy = java_output
            .join("native")
            .join(packaged_output.host_target.canonical_name())
            .join(&stale_shared_library_name);
        remove_file_if_exists(&structured_copy)?;

        if current_host == Some(packaged_output.host_target) {
            remove_file_if_exists(&java_output.join(stale_shared_library_name))?;
        }
    }

    Ok(())
}

fn remove_directory_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CliError::WriteFailed {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        remove_file_if_exists, remove_stale_flat_jvm_outputs_if_current_host_unrequested,
        remove_stale_requested_jvm_shared_library_copies_after_success,
    };
    use crate::config::Config;
    use crate::pack::java::link::{JvmNativePackageLayout, JvmPackagedNativeOutput};
    use crate::target::JavaHostTarget;

    fn temporary_directory(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    #[test]
    fn remove_file_if_exists_deletes_existing_file() {
        let temp_root = temporary_directory("boltffi-remove-file-test");
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let file_path = temp_root.join("stale.dylib");
        fs::write(&file_path, []).expect("write temp file");

        remove_file_if_exists(&file_path).expect("remove stale file");

        assert!(!file_path.exists());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn removes_stale_requested_shared_library_copies_only_after_success() {
        let current_host = JavaHostTarget::current();
        let requested_host = current_host.unwrap_or(JavaHostTarget::DarwinArm64);
        let temp_root = temporary_directory("boltffi-java-requested-shared-cleanup");
        let native_output = temp_root
            .join("native")
            .join(requested_host.canonical_name());
        fs::create_dir_all(&native_output).expect("create structured output dir");

        let structured_shared = native_output.join(requested_host.shared_library_filename("demo"));
        fs::write(&structured_shared, []).expect("write structured shared copy");

        let flat_shared = temp_root.join(requested_host.shared_library_filename("demo"));
        if current_host == Some(requested_host) {
            fs::write(&flat_shared, []).expect("write flat shared copy");
        }

        remove_stale_requested_jvm_shared_library_copies_after_success(
            &temp_root,
            &[JvmPackagedNativeOutput {
                host_target: requested_host,
                has_shared_library_copy: false,
                jni_library_path: temp_root.join(requested_host.jni_library_filename("demo")),
                shared_library_path: None,
                debug_info_sidecars: Vec::new(),
            }],
            "demo",
        )
        .expect("cleanup stale requested shared copies");

        assert!(!structured_shared.exists());
        if current_host == Some(requested_host) {
            assert!(!flat_shared.exists());
        }

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn removes_stale_flat_jvm_outputs_when_current_host_is_not_requested() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let requested_other_host = [
            JavaHostTarget::DarwinArm64,
            JavaHostTarget::DarwinX86_64,
            JavaHostTarget::LinuxX86_64,
            JavaHostTarget::LinuxAarch64,
            JavaHostTarget::WindowsX86_64,
        ]
        .into_iter()
        .find(|target| *target != current_host)
        .expect("alternate host");
        let temp_root = temporary_directory("boltffi-java-flat-cleanup");
        fs::create_dir_all(&temp_root).expect("create temp dir");

        let jni_copy = temp_root.join(current_host.jni_library_filename("demo"));
        let shared_copy = temp_root.join(current_host.shared_library_filename("demo"));
        fs::write(&jni_copy, []).expect("write stale jni");
        fs::write(&shared_copy, []).expect("write stale shared");

        remove_stale_flat_jvm_outputs_if_current_host_unrequested(
            &temp_root,
            Some(current_host),
            &[requested_other_host],
            "demo",
        )
        .expect("cleanup stale outputs");

        assert!(!jni_copy.exists());
        assert!(!shared_copy.exists());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn removes_stale_structured_jvm_outputs_when_host_matrix_is_narrowed() {
        let temp_root = temporary_directory("boltffi-java-structured-cleanup");
        let darwin_dir = temp_root.join(JavaHostTarget::DarwinArm64.canonical_name());
        let linux_dir = temp_root.join(JavaHostTarget::LinuxX86_64.canonical_name());
        fs::create_dir_all(&darwin_dir).expect("create darwin dir");
        fs::create_dir_all(&linux_dir).expect("create linux dir");
        let requested_jni = darwin_dir.join("libdemo_jni.dylib");
        let stale_jni = linux_dir.join("libdemo_jni.so");
        fs::write(&requested_jni, b"requested native library").unwrap();
        fs::write(&stale_jni, b"stale native library").unwrap();

        let config: Config = toml::from_str("[package]\nname = \"demo\"").unwrap();
        let layout = JvmNativePackageLayout::kotlin_desktop(
            &config,
            temp_root.join("jni"),
            "demo",
            temp_root.clone(),
        )
        .unwrap();
        layout
            .remove_stale_host_artifacts(&[JavaHostTarget::DarwinArm64], "demo")
            .expect("cleanup stale structured outputs");

        assert!(darwin_dir.exists());
        assert!(linux_dir.exists());
        assert!(requested_jni.is_file());
        assert!(!stale_jni.exists());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn preserves_requested_structured_jvm_outputs() {
        let temp_root = temporary_directory("boltffi-java-structured-preserve");
        let darwin_dir = temp_root.join(JavaHostTarget::DarwinArm64.canonical_name());
        let linux_dir = temp_root.join(JavaHostTarget::LinuxX86_64.canonical_name());
        fs::create_dir_all(&darwin_dir).expect("create darwin dir");
        fs::create_dir_all(&linux_dir).expect("create linux dir");

        let config: Config = toml::from_str("[package]\nname = \"demo\"").unwrap();
        let layout = JvmNativePackageLayout::kotlin_desktop(
            &config,
            temp_root.join("jni"),
            "demo",
            temp_root.clone(),
        )
        .unwrap();
        layout
            .remove_stale_host_artifacts(
                &[JavaHostTarget::DarwinArm64, JavaHostTarget::LinuxX86_64],
                "demo",
            )
            .expect("preserve structured outputs");

        assert!(darwin_dir.exists());
        assert!(linux_dir.exists());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn stale_host_cleanup_preserves_other_packages_and_application_resources() {
        let directory = tempfile::tempdir().expect("application resource directory");
        let windows = directory.path().join("windows-x86_64");
        let darwin = directory.path().join("darwin-arm64");
        fs::create_dir_all(windows.join("images")).expect("application resources");
        fs::create_dir_all(&darwin).expect("requested host output");
        let stale_jni = windows.join("journey_bindings_jni.dll");
        let stale_rust = windows.join("journey_native.dll");
        let stale_pdb = windows.join("journey_bindings_jni.pdb");
        let other_package = windows.join("maps_jni.dll");
        let other_pdb = windows.join("maps_jni.pdb");
        let application_resource = windows.join("images/splash.png");
        let requested_jni = darwin.join("libjourney_bindings_jni.dylib");
        [
            &stale_jni,
            &stale_rust,
            &stale_pdb,
            &other_package,
            &other_pdb,
            &application_resource,
            &requested_jni,
        ]
        .into_iter()
        .for_each(|path| fs::write(path, b"resource content").expect("resource file"));
        let stale_dsym = directory
            .path()
            .join("darwin-x86_64/libjourney_bindings_jni.dylib.dSYM");
        let other_dsym = directory.path().join("darwin-x86_64/libmaps.dylib.dSYM");
        [&stale_dsym, &other_dsym].into_iter().for_each(|path| {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("symbols"), b"debug symbols").unwrap();
        });

        let config: Config = toml::from_str("[package]\nname = \"journey-bindings\"").unwrap();
        let layout = JvmNativePackageLayout::kotlin_desktop(
            &config,
            directory.path().join("jni"),
            "journey_native",
            directory.path().to_path_buf(),
        )
        .unwrap();
        layout
            .remove_stale_host_artifacts(&[JavaHostTarget::DarwinArm64], "journey_native")
            .expect("clean stale package artifacts");

        assert_eq!(fs::read(other_package).unwrap(), b"resource content");
        assert_eq!(fs::read(other_pdb).unwrap(), b"resource content");
        assert_eq!(
            fs::read(other_dsym.join("symbols")).unwrap(),
            b"debug symbols"
        );
        assert_eq!(fs::read(application_resource).unwrap(), b"resource content");
        assert!(requested_jni.is_file());
        assert!(!stale_jni.exists());
        assert!(!stale_rust.exists());
        assert!(!stale_pdb.exists());
        assert!(!stale_dsym.exists());
    }

    #[test]
    #[cfg(unix)]
    fn stale_host_cleanup_preserves_symlinked_application_resources() {
        let resources = tempfile::tempdir().expect("application resources");
        let shared_directory = tempfile::tempdir().expect("shared native libraries");
        let shared_library = shared_directory.path().join("demo_jni.dll");
        fs::write(&shared_library, b"another application's native library").unwrap();
        let linked_host = resources.path().join("windows-x86_64");
        std::os::unix::fs::symlink(shared_directory.path(), &linked_host).unwrap();
        let config: Config = toml::from_str("[package]\nname = \"demo\"").unwrap();
        let layout = JvmNativePackageLayout::kotlin_desktop(
            &config,
            resources.path().join("jni"),
            "demo",
            resources.path().to_path_buf(),
        )
        .unwrap();

        layout
            .remove_stale_host_artifacts(&[JavaHostTarget::DarwinArm64], "demo")
            .expect("preserve external resources");

        assert!(linked_host.is_symlink());
        assert_eq!(
            fs::read(shared_library).unwrap(),
            b"another application's native library"
        );
    }

    #[test]
    fn preserves_flat_jvm_outputs_when_current_host_is_requested() {
        let current_host = JavaHostTarget::current().expect("supported test host");
        let temp_root = temporary_directory("boltffi-java-flat-preserve");
        fs::create_dir_all(&temp_root).expect("create temp dir");

        let jni_copy = temp_root.join(current_host.jni_library_filename("demo"));
        let shared_copy = temp_root.join(current_host.shared_library_filename("demo"));
        fs::write(&jni_copy, []).expect("write current jni");
        fs::write(&shared_copy, []).expect("write current shared");

        remove_stale_flat_jvm_outputs_if_current_host_unrequested(
            &temp_root,
            Some(current_host),
            &[current_host],
            "demo",
        )
        .expect("preserve current-host outputs");

        assert!(jni_copy.exists());
        assert!(shared_copy.exists());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }
}
