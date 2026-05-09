mod generator;
mod header;
mod languages;

use std::path::{Path, PathBuf};

use generator::{GenerateRequest, run_generator};
use header::HeaderGenerator;
use languages::{
    CSharpGenerator, DartGenerator, JavaGenerator, KotlinGenerator, PythonGenerator,
    SwiftGenerator, TypeScriptGenerator,
};

use crate::cli::Result;
use crate::config::{Config, Target};

pub enum GenerateTarget {
    Swift,
    Kotlin,
    Java,
    Header,
    Typescript,
    Dart,
    Python,
    CSharp,
    All,
}

pub struct GenerateOptions {
    pub target: GenerateTarget,
    pub output: Option<PathBuf>,
    pub experimental: bool,
}

pub fn run_generate_with_output(config: &Config, options: GenerateOptions) -> Result<()> {
    let request = GenerateRequest::for_current_crate(config, options.output);

    match options.target {
        GenerateTarget::Swift => run_generator::<SwiftGenerator>(&request, options.experimental),
        GenerateTarget::Kotlin => run_generator::<KotlinGenerator>(&request, options.experimental),
        GenerateTarget::Java => run_generator::<JavaGenerator>(&request, options.experimental),
        GenerateTarget::Header => run_generator::<HeaderGenerator>(&request, options.experimental),
        GenerateTarget::Typescript => {
            run_generator::<TypeScriptGenerator>(&request, options.experimental)
        }
        GenerateTarget::Dart => run_generator::<DartGenerator>(&request, options.experimental),
        GenerateTarget::Python => run_generator::<PythonGenerator>(&request, options.experimental),
        GenerateTarget::CSharp => run_generator::<CSharpGenerator>(&request, options.experimental),
        GenerateTarget::All => {
            if config.should_process(Target::Swift, options.experimental) {
                run_generator::<SwiftGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Kotlin, options.experimental) {
                run_generator::<KotlinGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Java, options.experimental) {
                run_generator::<JavaGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Header, options.experimental) {
                run_generator::<HeaderGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::TypeScript, options.experimental) {
                run_generator::<TypeScriptGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Dart, options.experimental) {
                run_generator::<DartGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Python, options.experimental) {
                run_generator::<PythonGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::CSharp, options.experimental) {
                run_generator::<CSharpGenerator>(&request, options.experimental)?;
            }

            Ok(())
        }
    }
}

pub fn run_generate_java_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    JavaGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

pub fn run_generate_python_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    PythonGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

pub fn run_generate_csharp_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    CSharpGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_bindgen::render::python::PythonRuntimeVersion;

    use super::languages::PythonGenerator;
    use crate::config::Config;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    fn demo_source_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo")
    }

    #[test]
    fn python_generate_writes_python_package_sources() {
        let output_directory = unique_temp_dir("boltffi-python-generate-test");
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.python]
enabled = true
"#,
        );

        PythonGenerator::generate_from_source_directory(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
        )
        .expect("python generate should succeed");

        let generated_init_path = output_directory.join("demo/__init__.py");
        let generated_stub_path = output_directory.join("demo/__init__.pyi");
        let generated_native_path = output_directory.join("demo/_native.c");
        let generated_pyproject_path = output_directory.join("pyproject.toml");
        let generated_setup_path = output_directory.join("setup.py");
        let generated_init = fs::read_to_string(&generated_init_path)
            .expect("generated python init should be readable");
        let generated_stub = fs::read_to_string(&generated_stub_path)
            .expect("generated python typing stub should be readable");
        let generated_native = fs::read_to_string(&generated_native_path)
            .expect("generated native bridge should be readable");
        let generated_pyproject = fs::read_to_string(&generated_pyproject_path)
            .expect("generated pyproject should be readable");
        let generated_setup = fs::read_to_string(&generated_setup_path)
            .expect("generated setup.py should be readable");
        let minimum_python_version_requirement =
            PythonRuntimeVersion::minimum_supported().package_requirement();

        assert!(generated_init.contains("from pathlib import Path"));
        assert!(generated_init.contains("from . import _native"));
        assert!(generated_init.contains("_native._initialize_loader"));
        assert!(generated_init.contains("__all__ = ["));
        assert!(generated_init.contains("PACKAGE_NAME = \"demo\""));
        assert!(generated_stub.contains("MODULE_NAME: str"));
        assert!(generated_stub.contains("def echo_i32"));
        assert!(generated_pyproject.contains("setuptools.build_meta"));
        assert!(generated_setup.contains("Extension("));
        assert!(generated_setup.contains("\"demo._native\""));
        assert!(generated_setup.contains(&format!(
            "python_requires={minimum_python_version_requirement:?}"
        )));
        assert!(generated_native.contains("boltffi_python_symbol_echo_i32_fn"));
        assert!(generated_native.contains("boltffi_python_initialize_loader"));
        assert!(generated_native.contains("PyInit__native"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }
}
