use std::path::PathBuf;

use boltffi_backend::target::kmp::KMP_SUPPORT_REPORT_FILE;

use crate::cli::Result;
use crate::config::Config;
use crate::pack::android::AndroidPackageLayout;
use crate::pack::java::link::JvmNativePackageLayout;

/// Generated Kotlin Multiplatform project paths used while packaging KMP output.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct KmpPackageLayout {
    output_root: PathBuf,
    android_jni_dir: PathBuf,
    android_jnilibs_dir: PathBuf,
    jvm_jni_dir: PathBuf,
    jvm_native_resource_root: PathBuf,
    support_report_path: PathBuf,
}

impl KmpPackageLayout {
    /// Builds the packaging layout from the configured KMP output directory.
    pub(crate) fn from_config(config: &Config) -> Self {
        let output_root = config.kotlin_multiplatform_output();
        Self {
            android_jni_dir: output_root.join("src/androidMain/c"),
            android_jnilibs_dir: output_root.join("src/androidMain/jniLibs"),
            jvm_jni_dir: output_root.join("src/jvmMain/c"),
            jvm_native_resource_root: output_root.join("src/jvmMain/resources/native"),
            support_report_path: output_root.join(KMP_SUPPORT_REPORT_FILE),
            output_root,
        }
    }

    /// Returns the generated KMP project root.
    pub(crate) fn output_root(&self) -> &PathBuf {
        &self.output_root
    }

    /// Returns the JVM source-set directory containing generated JNI C glue.
    pub(crate) fn jvm_jni_dir(&self) -> &PathBuf {
        &self.jvm_jni_dir
    }

    /// Returns the Android source-set directory containing generated JNI C glue.
    pub(crate) fn android_jni_dir(&self) -> &PathBuf {
        &self.android_jni_dir
    }

    /// Returns the JVM native resources root used for packaged desktop libraries.
    pub(crate) fn jvm_native_resource_root(&self) -> &PathBuf {
        &self.jvm_native_resource_root
    }

    /// Returns the generated KMP support metadata path.
    pub(crate) fn support_report_path(&self) -> &PathBuf {
        &self.support_report_path
    }

    /// Creates the Android package layout used by the shared Android linker.
    pub(crate) fn android_native_layout(&self, header_name: &str) -> AndroidPackageLayout {
        AndroidPackageLayout {
            jni_glue_path: self.android_jni_dir.join("jni_glue.c"),
            header_include_dir: self.android_jni_dir.clone(),
            header_name: header_name.to_string(),
            jnilibs_path: self.android_jnilibs_dir.clone(),
        }
    }

    /// Creates the JVM native package layout used by the shared JVM linker.
    pub(crate) fn jvm_native_layout(
        &self,
        config: &Config,
        header_name: &str,
    ) -> Result<JvmNativePackageLayout> {
        JvmNativePackageLayout::kotlin_desktop(
            config,
            self.jvm_jni_dir.clone(),
            header_name,
            self.jvm_native_resource_root.clone(),
        )
    }
}
