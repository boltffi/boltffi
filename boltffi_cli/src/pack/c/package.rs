use std::path::Path;
use std::process::Command;

use askama::Template;

use crate::build::native_link::NativeLinkMetadata;
use crate::cargo::SelectedLibrary;
use crate::cli::{CliError, Result};
use crate::config::Config;
use crate::target::NativeHostPlatform;

pub struct CPackage {
    name: String,
    version: String,
    description: String,
    artifact_name: String,
    platform: NativeHostPlatform,
}

#[derive(Template)]
#[template(path = "CPackageConfig.cmake", escape = "none")]
struct CMakePackage<'a> {
    name: &'a str,
    version: String,
    shared_library: String,
    static_library: String,
    import_library: Option<String>,
    no_soname: bool,
    static_dependencies: String,
    static_link_options: String,
}

#[derive(Template)]
#[template(path = "CPackageConfigVersion.cmake", escape = "none")]
struct CMakePackageVersion {
    version: String,
    pointer_size: usize,
}

#[derive(Template)]
#[template(path = "CPackage.pc", escape = "none")]
struct PkgConfigPackage<'a> {
    name: &'a str,
    description: &'a str,
    version: &'a str,
    library_directory: &'a str,
    libraries: String,
    private_libraries: &'a str,
}

impl CPackage {
    pub fn new(
        config: &Config,
        library: &SelectedLibrary,
        platform: NativeHostPlatform,
    ) -> Result<Self> {
        let name = config.library_name();
        let version = config
            .package
            .version
            .as_deref()
            .unwrap_or(library.package_version());
        if !name
            .starts_with(|character: char| character.is_ascii_alphanumeric() || character == '_')
            || !name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            })
        {
            return Err(CliError::CommandFailed {
                command: "C package names must start with an ASCII letter, digit, or underscore and contain only ASCII letters, digits, underscores, hyphens, or dots".to_owned(),
                status: None,
            });
        }
        if version.is_empty()
            || !version.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
            })
        {
            return Err(CliError::CommandFailed {
                command: "C package version must be a nonempty version without whitespace or special characters".to_owned(),
                status: None,
            });
        }
        Ok(Self {
            name: name.to_owned(),
            version: version.to_owned(),
            description: config
                .package
                .description
                .as_deref()
                .unwrap_or(name)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            artifact_name: library.artifact_name().to_owned(),
            platform,
        })
    }

    pub fn write_metadata(&self, output: &Path, native_link: &NativeLinkMetadata) -> Result<()> {
        let (link_options, link_libraries): (Vec<_>, Vec<_>) = native_link
            .native_static_libraries
            .iter()
            .map(String::as_str)
            .partition(|library| {
                matches!(
                    self.platform,
                    NativeHostPlatform::WindowsX86_64 | NativeHostPlatform::WindowsAarch64
                ) && library.starts_with('/')
            });
        let static_dependencies = Self::cmake_static_dependencies(&link_libraries)?;
        let static_link_options = link_options
            .into_iter()
            .map(Self::cmake_quote)
            .collect::<Vec<_>>()
            .join(" ");
        let native_libraries = native_link.native_static_libraries.join(" ");
        let cmake = CMakePackage {
            name: &self.name,
            version: Self::cmake_quote(&self.version),
            shared_library: self.platform.shared_library_filename(&self.artifact_name),
            static_library: self.platform.static_library_filename(&self.artifact_name),
            import_library: self.platform.import_library_filename(&self.artifact_name),
            no_soname: matches!(
                self.platform,
                NativeHostPlatform::LinuxX86_64 | NativeHostPlatform::LinuxAarch64
            ),
            static_dependencies,
            static_link_options,
        };
        let cmake_version = CMakePackageVersion {
            version: Self::cmake_quote(&self.version),
            pointer_size: std::mem::size_of::<usize>(),
        };
        let static_link_flags = format!("-L${{libdir}}/static {native_libraries}");
        let shared = PkgConfigPackage {
            name: &self.name,
            description: &self.description,
            version: &self.version,
            library_directory: "lib",
            libraries: if cmake.import_library.is_some() {
                format!("-L${{libdir}} -l{}.dll", self.artifact_name)
            } else {
                format!("-L${{libdir}} -l{}", self.artifact_name)
            },
            private_libraries: &static_link_flags,
        };
        let static_package = PkgConfigPackage {
            library_directory: "lib/static",
            libraries: format!("-L${{libdir}} -l{} {native_libraries}", self.artifact_name),
            private_libraries: "",
            ..shared
        };
        let files = [
            (
                format!("lib/cmake/{0}/{0}Config.cmake", self.name),
                cmake.render(),
            ),
            (
                format!("lib/cmake/{0}/{0}ConfigVersion.cmake", self.name),
                cmake_version.render(),
            ),
            (format!("lib/pkgconfig/{}.pc", self.name), shared.render()),
            (
                format!("lib/pkgconfig/{}-static.pc", self.name),
                static_package.render(),
            ),
        ];
        files.into_iter().try_for_each(|(relative_path, content)| {
            let path = output.join(relative_path);
            let content = content.map_err(|source| CliError::CommandFailed {
                command: format!("generate C package metadata: {source}"),
                status: None,
            })?;
            let parent = path.parent().expect("package metadata has a directory");
            std::fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
                path: parent.to_path_buf(),
                source,
            })?;
            std::fs::write(&path, format!("{content}\n"))
                .map_err(|source| CliError::WriteFailed { path, source })
        })
    }

    pub fn relocate_shared_library(&self, output: &Path) -> Result<()> {
        if !matches!(
            self.platform,
            NativeHostPlatform::DarwinArm64 | NativeHostPlatform::DarwinX86_64
        ) {
            return Ok(());
        }
        let filename = self.platform.shared_library_filename(&self.artifact_name);
        let library = output.join("lib").join(&filename);
        [
            (
                "install_name_tool",
                vec![
                    "-id".to_owned(),
                    format!("@rpath/{filename}"),
                    library.display().to_string(),
                ],
            ),
            (
                "codesign",
                vec![
                    "--force".to_owned(),
                    "--sign".to_owned(),
                    "-".to_owned(),
                    library.display().to_string(),
                ],
            ),
        ]
        .into_iter()
        .try_for_each(|(program, arguments)| {
            let result = Command::new(program)
                .args(arguments)
                .output()
                .map_err(|source| CliError::CommandFailed {
                    command: format!("{program}: {source}"),
                    status: None,
                })?;
            if result.status.success() {
                Ok(())
            } else {
                Err(CliError::CommandFailed {
                    command: format!("{program}: {}", String::from_utf8_lossy(&result.stderr)),
                    status: result.status.code(),
                })
            }
        })
    }

    fn cmake_static_dependencies(libraries: &[&str]) -> Result<String> {
        let mut libraries = libraries.iter();
        let mut dependencies = Vec::new();
        while let Some(library) = libraries.next() {
            let dependency = if *library == "-framework" {
                let framework = libraries.next().ok_or_else(|| CliError::CommandFailed {
                    command: "native static link metadata has -framework without a framework name"
                        .to_owned(),
                    status: None,
                })?;
                format!("-framework {framework}")
            } else {
                (*library).to_owned()
            };
            dependencies.push(Self::cmake_quote(&dependency));
        }
        Ok(dependencies.join(" "))
    }

    fn cmake_quote(value: &str) -> String {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
                .replace(';', "\\;")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::CPackage;
    use crate::build::native_link::NativeLinkMetadata;
    use crate::cargo::SelectedLibrary;
    use crate::config::Config;
    use crate::target::NativeHostPlatform;

    #[test]
    fn package_metadata_keeps_selected_binary_names_and_cargo_version() {
        let config: Config = toml::from_str("[package]\nname = \"public-api\"\n").expect("config");
        let library = SelectedLibrary::fixture("rust-package", "/source/Cargo.toml", "native_api");
        let package =
            CPackage::new(&config, &library, NativeHostPlatform::WindowsX86_64).expect("C package");
        let directory = tempfile::tempdir().expect("package directory");
        package
            .write_metadata(
                directory.path(),
                &NativeLinkMetadata {
                    native_static_libraries: vec![
                        "ws2_32.lib".to_owned(),
                        "userenv.lib".to_owned(),
                        "/defaultlib:msvcrt".to_owned(),
                    ],
                    native_link_search_paths: vec!["native=/source/target/debug/build".to_owned()],
                },
            )
            .expect("package metadata");

        let cmake = std::fs::read_to_string(
            directory
                .path()
                .join("lib/cmake/public-api/public-apiConfig.cmake"),
        )
        .expect("CMake package");
        assert!(cmake.contains("add_library(public-api::public-api SHARED IMPORTED)"));
        assert!(cmake.contains("/lib/native_api.dll\""));
        assert!(cmake.contains("IMPORTED_IMPLIB"));
        assert!(cmake.contains("INTERFACE_LINK_OPTIONS \"/defaultlib:msvcrt\""));
        assert!(cmake.contains("set(public-api_VERSION \"0.1.0\")"));
        assert!(!cmake.contains("/source/"));

        let pkg_config =
            std::fs::read_to_string(directory.path().join("lib/pkgconfig/public-api-static.pc"))
                .expect("static pkg-config package");
        assert!(pkg_config.contains("ws2_32.lib userenv.lib"));
        assert!(pkg_config.contains("prefix=${pcfiledir}/../.."));
        assert!(pkg_config.contains("Version: 0.1.0"));
        assert!(pkg_config.contains("libdir=${prefix}/lib/static"));
        assert!(pkg_config.contains("Libs: -L${libdir} -lnative_api "));
        assert!(!pkg_config.contains("/source/"));
    }

    #[test]
    fn configured_package_version_overrides_selected_cargo_version() {
        let config: Config =
            toml::from_str("[package]\nname = \"demo\"\nversion = \"2.3.4\"\n").expect("config");
        let library = SelectedLibrary::fixture("demo", "/source/Cargo.toml", "demo");
        let package =
            CPackage::new(&config, &library, NativeHostPlatform::LinuxX86_64).expect("C package");
        assert_eq!(package.version, "2.3.4");
    }

    #[test]
    fn package_names_cannot_escape_the_output_directory() {
        let library = SelectedLibrary::fixture("demo", "/source/Cargo.toml", "demo");
        ["..", ".", "../demo", "demo;injected", "${demo}"]
            .iter()
            .for_each(|name| {
                let mut config: Config =
                    toml::from_str("[package]\nname = \"demo\"\n").expect("config");
                config.package.name = (*name).to_owned();
                assert!(CPackage::new(&config, &library, NativeHostPlatform::LinuxX86_64).is_err());
            });
    }

    #[test]
    fn static_link_dependencies_preserve_framework_pairs_and_library_order() {
        let libraries = [
            "-framework",
            "Security",
            "-lc",
            "-framework",
            "CoreFoundation",
            "-lc",
        ];
        assert_eq!(
            CPackage::cmake_static_dependencies(&libraries).expect("link dependencies"),
            "\"-framework Security\" \"-lc\" \"-framework CoreFoundation\" \"-lc\""
        );
    }

    #[test]
    fn framework_without_a_name_is_rejected() {
        assert!(CPackage::cmake_static_dependencies(&["-framework"]).is_err());
    }
}
