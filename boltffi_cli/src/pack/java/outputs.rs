use std::path::Path;

use crate::cli::{CliError, Result};
use crate::target::JavaHostTarget;

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

/// Remove stale files from a single arch's `native/<arch>/` directory before it
/// is rebuilt, leaving sibling arches untouched. Used by the JNI compile step so
/// that an incremental, per-arch pack only ever cleans the subdir it owns.
pub(crate) fn clean_structured_jvm_output_dir(native_arch_dir: &Path) -> Result<()> {
    match std::fs::remove_dir_all(native_arch_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CliError::WriteFailed {
            path: native_arch_dir.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        clean_structured_jvm_output_dir, remove_file_if_exists,
        remove_stale_flat_jvm_outputs_if_current_host_unrequested,
        remove_stale_requested_jvm_shared_library_copies_after_success,
    };
    use crate::pack::java::link::JvmPackagedNativeOutput;
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
    fn clean_structured_jvm_output_dir_removes_only_the_named_arch() {
        let temp_root = temporary_directory("boltffi-java-structured-clean");
        let darwin_dir = temp_root.join(JavaHostTarget::DarwinArm64.canonical_name());
        let linux_dir = temp_root.join(JavaHostTarget::LinuxX86_64.canonical_name());
        fs::create_dir_all(&darwin_dir).expect("create darwin dir");
        fs::create_dir_all(&linux_dir).expect("create linux dir");
        fs::write(linux_dir.join("stale.so"), []).expect("write stale lib");

        clean_structured_jvm_output_dir(&linux_dir).expect("clean only the rebuilt arch");

        // Only the named arch is cleaned; sibling arches survive.
        assert!(!linux_dir.exists());
        assert!(darwin_dir.exists());

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
    }

    #[test]
    fn clean_structured_jvm_output_dir_is_a_noop_when_absent() {
        let temp_root = temporary_directory("boltffi-java-structured-clean-absent");
        fs::create_dir_all(&temp_root).expect("create temp dir");
        let missing = temp_root.join(JavaHostTarget::LinuxAarch64.canonical_name());

        clean_structured_jvm_output_dir(&missing).expect("missing dir is not an error");

        fs::remove_dir_all(&temp_root).expect("cleanup temp dir");
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
