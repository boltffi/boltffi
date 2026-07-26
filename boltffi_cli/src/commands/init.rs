use std::path::{Path, PathBuf};

use crate::cli::Result;
use crate::config::{
    AndroidConfig, AndroidPackConfig, AppleConfig, CSharpConfig, CargoConfig, Config, DartConfig,
    DebugSymbolsConfig, ErrorStyle, HeaderConfig, JavaConfig, KotlinConfig, KotlinFactoryStyle,
    KotlinMultiplatformConfig, PackageConfig, PythonConfig, SpmConfig, SwiftConfig, TargetsConfig,
    WasmConfig, XcframeworkConfig,
};

pub struct InitOptions {
    pub name: Option<String>,
    pub path: PathBuf,
}

pub fn run_init(options: InitOptions) -> Result<PathBuf> {
    let config_path = options.path.join("boltffi.toml");

    if config_path.exists() {
        println!("boltffi.toml already exists");
        return Ok(config_path);
    }

    let package_name = options
        .name
        .or_else(|| detect_package_name(&options.path))
        .unwrap_or_else(|| "mylib".to_string());

    let config = create_default_config(&package_name);
    config.save(&config_path)?;

    println!("Created boltffi.toml");
    println!();
    println!("Next steps:");
    println!("  1. boltffi check     # verify your environment");
    println!("  2. boltffi generate  # generate bindings");
    println!("  3. boltffi build     # compile for targets");
    println!("  4. boltffi pack apple  # package Apple artifacts");

    Ok(config_path)
}

fn detect_package_name(path: &Path) -> Option<String> {
    let cargo_toml = path.join("Cargo.toml");

    if !cargo_toml.exists() {
        return None;
    }

    std::fs::read_to_string(&cargo_toml)
        .ok()
        .and_then(|content| toml::from_str::<toml::Value>(&content).ok())
        .and_then(|value| {
            value
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .map(str::to_string)
        })
}

fn create_default_config(package_name: &str) -> Config {
    let module_name = to_pascal_case(package_name);
    let normalized_kotlin_name = package_name.replace('-', "_");

    Config {
        experimental: Vec::new(),
        cargo: CargoConfig::default(),
        package: PackageConfig {
            name: package_name.to_string(),
            crate_name: None,
            version: None,
            description: None,
            license: None,
            repository: None,
        },
        targets: TargetsConfig {
            apple: AppleConfig {
                enabled: true,
                output: PathBuf::from("dist/apple"),
                deployment_target: "16.0".to_string(),
                include_macos: false,
                ios_architectures: None,
                simulator_architectures: None,
                macos_architectures: None,
                swift: SwiftConfig {
                    module_name: Some(module_name),
                    output: None,
                    ffi_module_name: None,
                    tools_version: Some("5.9".to_string()),
                    error_style: ErrorStyle::default(),
                    type_mappings: Default::default(),
                },
                xcframework: XcframeworkConfig {
                    output: None,
                    name: None,
                },
                spm: SpmConfig {
                    output: None,
                    distribution: Default::default(),
                    repo_url: None,
                    layout: Default::default(),
                    package_name: None,
                    wrapper_sources: None,
                    skip_package_swift: false,
                },
                debug_symbols: DebugSymbolsConfig::default(),
            },
            android: AndroidConfig {
                enabled: true,
                output: PathBuf::from("dist/android"),
                min_sdk: 24,
                ndk_version: None,
                architectures: None,
                kotlin: KotlinConfig {
                    package: Some(format!("com.example.{}", normalized_kotlin_name)),
                    output: None,
                    module_name: None,
                    library_name: None,
                    desktop_loader: Default::default(),
                    desktop_pack: Default::default(),
                    api_style: Default::default(),
                    error_style: ErrorStyle::default(),
                    factory_style: KotlinFactoryStyle::default(),
                    type_mappings: Default::default(),
                },
                header: HeaderConfig { output: None },
                pack: AndroidPackConfig { output: None },
                debug_symbols: DebugSymbolsConfig::default(),
            },
            kotlin_multiplatform: KotlinMultiplatformConfig::default(),
            wasm: WasmConfig::default(),
            java: JavaConfig::default(),
            dart: DartConfig::default(),
            python: PythonConfig::default(),
            csharp: CSharpConfig::default(),
        },
    }
}

fn to_pascal_case(input: &str) -> String {
    input
        .split(['_', '-'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{InitOptions, run_init};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn run_init_writes_requested_config_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-init-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp root");
        fs::write(
            temp_root.join("Cargo.toml"),
            "[package]\nname = \"demo_lib\"\nversion = \"0.1.0\"\n",
        )
        .expect("write cargo toml");

        let config_path = temp_root.join("boltffi.toml");
        let written_path = run_init(InitOptions {
            name: None,
            path: temp_root.clone(),
        })
        .expect("init should succeed");

        assert_eq!(written_path, config_path);
        assert!(config_path.exists());
        let content = fs::read_to_string(&config_path).expect("read config");
        assert!(content.contains("name = \"demo_lib\""));
        assert!(content.contains("[targets.kotlin_multiplatform]"));
        assert!(content.contains("[targets.python]"));
        assert!(content.contains("[targets.python.wheel]"));

        fs::remove_dir_all(temp_root).expect("cleanup temp root");
    }

    #[test]
    fn run_init_falls_back_when_cargo_manifest_has_no_package_table() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("boltffi-init-workspace-test-{unique}"));
        fs::create_dir_all(&temp_root).expect("create temp root");
        fs::write(temp_root.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("write workspace cargo toml");

        let config_path = temp_root.join("boltffi.toml");
        run_init(InitOptions {
            name: None,
            path: temp_root.clone(),
        })
        .expect("init should succeed");

        let content = fs::read_to_string(&config_path).expect("read config");
        assert!(content.contains("name = \"mylib\""));

        fs::remove_dir_all(temp_root).expect("cleanup temp root");
    }
}
