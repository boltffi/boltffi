use std::path::{Component, Path, PathBuf};

use boltffi_backend::target::java::JavaVersion;

use crate::{
    cli::{CliError, Result},
    config::Config,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Jvm,
    Android,
}

pub struct Plan {
    output: PathBuf,
    platform: Platform,
    version: JavaVersion,
}

impl Plan {
    pub fn resolve(config: &Config, output: Option<PathBuf>) -> Result<Self> {
        let output = output.unwrap_or_else(|| match config.is_java_jvm_enabled() {
            true => config.java_jvm_output(),
            false => config.java_android_output(),
        });
        let platform = Platform::resolve(config, &output)?;
        let version = Self::configured_version(config)?;
        Ok(Self {
            output,
            platform,
            version,
        })
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    pub const fn platform(&self) -> Platform {
        self.platform
    }

    pub const fn version(&self) -> JavaVersion {
        self.version
    }

    fn configured_version(config: &Config) -> Result<JavaVersion> {
        let release = config.java_min_version().unwrap_or(8);
        JavaVersion::new(release).ok_or_else(|| CliError::CommandFailed {
            command: format!("targets.java.min_version must be between 8 and 26, got {release}"),
            status: None,
        })
    }
}

impl Platform {
    pub fn resolve(config: &Config, output_directory: &Path) -> Result<Self> {
        match (
            config.is_java_jvm_enabled(),
            config.is_java_android_enabled(),
        ) {
            (true, false) => return Ok(Self::Jvm),
            (false, true) => return Ok(Self::Android),
            (false, false) => {
                return Err(CliError::CommandFailed {
                    command:
                        "both targets.java.jvm.enabled and targets.java.android.enabled are false"
                            .to_owned(),
                    status: None,
                });
            }
            (true, true) => {}
        }

        let output_directory = Self::normalized_output_path(output_directory)?;
        let jvm_output = Self::normalized_output_path(&config.java_jvm_output())?;
        let android_output = Self::normalized_output_path(&config.java_android_output())?;

        match (
            output_directory == jvm_output,
            output_directory == android_output,
        ) {
            (true, false) => Ok(Self::Jvm),
            (false, true) => Ok(Self::Android),
            (true, true) | (false, false) => Err(CliError::CommandFailed {
                command: format!(
                    "Java platform is ambiguous for output {}; select either the configured JVM output {} or Android output {}",
                    output_directory.display(),
                    config.java_jvm_output().display(),
                    config.java_android_output().display(),
                ),
                status: None,
            }),
        }
    }

    pub const fn uses_desktop_loader(self) -> bool {
        matches!(self, Self::Jvm)
    }

    fn normalized_output_path(path: &Path) -> Result<PathBuf> {
        let absolute_path = match path.is_absolute() {
            true => path.to_path_buf(),
            false => std::env::current_dir()
                .map(|current_directory| current_directory.join(path))
                .map_err(|source| CliError::ReadFailed {
                    path: path.to_path_buf(),
                    source,
                })?,
        };

        Ok(absolute_path
            .components()
            .fold(PathBuf::new(), |mut normalized_path, component| {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if normalized_path.file_name().is_some() {
                            normalized_path.pop();
                        }
                    }
                    _ => normalized_path.push(component.as_os_str()),
                }
                normalized_path
            }))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::Config;

    use super::{Plan, Platform};

    fn config(source: &str) -> Config {
        let config: Config = toml::from_str(source).expect("valid Java config");
        config.validate().expect("valid Java configuration");
        config
    }

    #[test]
    fn resolves_enabled_platform_from_the_selected_output() {
        let config = config(
            r#"
[package]
name = "demo"

[targets.java.jvm]
enabled = true
output = "/tmp/boltffi-java-jvm"

[targets.java.android]
enabled = true
output = "/tmp/boltffi-java-android"
"#,
        );

        assert_eq!(
            Platform::resolve(&config, Path::new("/tmp/boltffi-java-jvm")).unwrap(),
            Platform::Jvm
        );
        assert_eq!(
            Platform::resolve(&config, Path::new("/tmp/boltffi-java-android")).unwrap(),
            Platform::Android
        );
    }

    #[test]
    fn ignores_the_disabled_platform_output() {
        let config = config(
            r#"
[package]
name = "demo"

[targets.java.jvm]
enabled = true
output = "/tmp/boltffi-java-jvm"

[targets.java.android]
enabled = false
output = "/tmp/boltffi-java-android"
"#,
        );

        assert_eq!(
            Platform::resolve(&config, Path::new("/tmp/boltffi-java-android")).unwrap(),
            Platform::Jvm
        );
    }

    #[test]
    fn rejects_an_ambiguous_custom_output() {
        let config = config(
            r#"
[package]
name = "demo"

[targets.java.jvm]
enabled = true
output = "/tmp/boltffi-java-jvm"

[targets.java.android]
enabled = true
output = "/tmp/boltffi-java-android"
"#,
        );

        assert!(Platform::resolve(&config, Path::new("/tmp/boltffi-java-custom")).is_err());
    }

    #[test]
    fn validates_the_configured_java_release_once() {
        let supported = config(
            r#"
[package]
name = "demo"

[targets.java]
min_version = 17
"#,
        );
        let unsupported = config(
            r#"
[package]
name = "demo"

[targets.java]
min_version = 7
"#,
        );

        assert_eq!(Plan::configured_version(&supported).unwrap().release(), 17);
        assert!(Plan::configured_version(&unsupported).is_err());
    }

    #[test]
    fn generation_plan_validates_java_before_external_work() {
        let disabled = config(
            r#"
[package]
name = "demo"
"#,
        );

        assert!(Plan::resolve(&disabled, None).is_err());
    }
}
