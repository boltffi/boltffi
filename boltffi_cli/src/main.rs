mod build;
mod cargo;
mod check;
mod cli;
mod commands;
mod config;
pub mod pack;
mod reporter;
mod target;
mod toolchain;

use clap::{Parser, Subcommand};
use std::{fmt, path::PathBuf};

use cli::{CliError, Result};
use commands::build::{BuildCommandOptions, BuildPlatform};
use commands::check::CheckOptions;
use commands::doctor::{ConfigSummary, DoctorOptions};
use commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use commands::init::InitOptions;
use commands::pack::{
    PackAllOptions, PackAndroidOptions, PackAppleOptions, PackCSharpOptions, PackCommand,
    PackDartOptions, PackExecutionOptions, PackJavaOptions, PackPythonOptions, PackWasmOptions,
    check_java_packaging_prereqs,
};
use commands::verify::VerifyOptions;
use commands::{run_build, run_check, run_doctor, run_init, run_pack, run_verify};
use config::{Config, Target};

#[derive(Parser)]
#[command(name = "boltffi")]
#[command(about = "BoltFFI - Rust FFI toolchain (Apple + Android + WASM)")]
#[command(
    after_help = "Examples:\n  boltffi init\n  boltffi check --apple\n  boltffi generate swift\n  boltffi build apple --release\n  boltffi build wasm --release\n  boltffi pack apple --layout bundled\n  boltffi pack wasm --release\n  boltffi pack csharp\n  boltffi --overlay boltffi.ci.toml pack android\n\nConfig:\n  boltffi reads ./boltffi.toml\n  Use --overlay PATH to load a merged overlay config on top of it\n  Settings live under [targets.apple.*], [targets.android.*], [targets.wasm.*], [targets.java.*], [targets.dart.*], [targets.python.*], and [targets.csharp.*]\n"
)]
#[command(version)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count, global = true, help = "Increase verbosity (-v, -vv)")]
    verbose: u8,

    #[arg(short, long, global = true, help = "Suppress all output")]
    quiet: bool,

    #[arg(
        long = "cargo-arg",
        global = true,
        action = clap::ArgAction::Append,
        allow_hyphen_values = true,
        value_name = "ARG",
        help = "Pass an argument to cargo invocations (repeatable)"
    )]
    cargo_args: Vec<String>,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Optional overlay config file merged on top of the base config"
    )]
    overlay: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Create boltffi.toml with sensible defaults")]
    Init {
        #[arg(long)]
        name: Option<String>,
    },

    #[command(
        about = "Check toolchain + required rust targets",
        long_about = "Check toolchain + required rust targets.\n\nIf no platform flags are provided, boltffi checks Apple, Android, and WASM.\n\nExamples:\n  boltffi check\n  boltffi check --apple\n  boltffi check --android\n  boltffi check --wasm\n  boltffi check --fix\n"
    )]
    Check {
        #[arg(long, help = "Install missing rust targets")]
        fix: bool,

        #[arg(long, help = "Check Apple (iOS/iOS-simulator) targets + Xcode tooling")]
        apple: bool,

        #[arg(long, help = "Check Android targets + NDK")]
        android: bool,

        #[arg(long, help = "Check WASM target")]
        wasm: bool,
    },

    #[command(
        about = "Print diagnostic environment info",
        long_about = "Print diagnostic environment info.\n\nExamples:\n  boltffi doctor\n  boltffi doctor --apple\n  boltffi doctor --android\n  boltffi doctor --wasm\n"
    )]
    Doctor {
        #[arg(long)]
        apple: bool,

        #[arg(long)]
        android: bool,

        #[arg(long)]
        wasm: bool,
    },

    #[command(
        about = "Generate bindings",
        long_about = "Generate bindings.\n\nExamples:\n  boltffi generate\n  boltffi generate swift\n  boltffi generate kotlin\n  boltffi generate header\n  boltffi generate python --experimental\n"
    )]
    Generate {
        #[arg(value_enum)]
        target: Option<GenerateTargetArg>,

        #[arg(
            short,
            long,
            help = "Override output directory (default comes from boltffi.toml)"
        )]
        output: Option<PathBuf>,

        #[arg(long, help = "Enable experimental targets/features")]
        experimental: bool,
    },

    #[command(
        about = "Build rust libraries for targets",
        long_about = "Build rust libraries for targets.\n\nExamples:\n  boltffi build\n  boltffi build apple\n  boltffi build android --release\n  boltffi build wasm --release\n"
    )]
    Build {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,

        #[arg(long)]
        release: bool,
    },

    #[command(
        about = "Package platform artifacts (xcframework/SPM/jniLibs/npm/NuGet)",
        long_about = "Package platform artifacts.\n\nExamples:\n  boltffi pack apple\n  boltffi pack apple --layout bundled\n  boltffi pack android --release\n  boltffi pack wasm --release\n  boltffi pack python --experimental\n  boltffi pack csharp\n"
    )]
    Pack {
        #[command(subcommand)]
        target: PackTargetArg,
    },

    #[command(about = "Run check/build/generate/pack in order")]
    Release {
        #[arg(value_enum)]
        platform: Option<BuildPlatformArg>,
    },

    #[command(about = "Verify a generated binding file")]
    Verify {
        path: PathBuf,

        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum GenerateTargetArg {
    #[value(help = "Generate Swift bindings")]
    Swift,
    #[value(help = "Generate Kotlin bindings + JNI glue")]
    Kotlin,
    #[value(help = "Generate Java bindings + JNI glue")]
    Java,
    #[value(help = "Generate C header")]
    Header,
    #[value(help = "Generate TypeScript bindings for WASM")]
    Typescript,
    #[value(help = "Generate experimental Dart bindings")]
    Dart,
    #[value(help = "Generate experimental Python bindings")]
    Python,
    #[value(help = "Generate C# bindings")]
    Csharp,
    #[value(help = "Generate all bindings")]
    All,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum BuildPlatformArg {
    #[value(help = "Build Apple targets (iOS + iOS-simulator, and macOS if enabled)")]
    Apple,
    #[value(help = "Build Android targets")]
    Android,
    #[value(help = "Build wasm target")]
    Wasm,
    #[value(help = "Build dart target")]
    Dart,
    #[value(help = "Build all configured targets")]
    All,
}

#[derive(Subcommand)]
enum PackTargetArg {
    #[command(about = "Package all enabled targets")]
    All {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,

        #[arg(long, help = "Include experimental targets/features")]
        experimental: bool,

        #[arg(
            long = "python",
            action = clap::ArgAction::Append,
            value_name = "PYTHON",
            help = "Python interpreter executable or path for Python packaging (repeatable)"
        )]
        python_interpreters: Vec<String>,
    },

    #[command(
        about = "Build + package Apple artifacts",
        long_about = "Build + package Apple artifacts.\n\nOutputs:\n  - xcframework: {targets.apple.xcframework.output}/{Name}.xcframework\n  - SwiftPM:      {targets.apple.spm.output}/Package.swift\n\nLayout:\n  bundled  -> one package with wrapper target\n  ffi-only -> standalone FFI package with Swift target\n  split    -> binary-only package (Swift bindings generated to targets.apple.swift.output)\n"
    )]
    Apple {
        #[arg(long)]
        release: bool,

        #[arg(long)]
        version: Option<String>,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,

        #[arg(long)]
        spm_only: bool,

        #[arg(long)]
        xcframework_only: bool,

        #[arg(long, value_enum)]
        layout: Option<PackLayoutArg>,
    },

    #[command(
        about = "Build + package Android artifacts",
        long_about = "Build + package Android artifacts.\n\nOutputs:\n  - Kotlin/JNI: {targets.android.kotlin.output}\n  - jniLibs:    {targets.android.pack.output}\n"
    )]
    Android {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },

    #[command(
        about = "Build + package WASM artifacts",
        long_about = "Build + package WASM artifacts.\n\nOutputs:\n  - wasm binary: {targets.wasm.npm.output}/{module_name}_bg.wasm\n  - JS/TS files: {targets.wasm.npm.output}\n  - npm metadata: package.json/README.md (when enabled)\n"
    )]
    Wasm {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },

    #[command(
        about = "Build + package Java artifacts",
        long_about = "Build + package Java artifacts.\n\nOutputs:\n  - Java bindings: {targets.java.jvm.output}\n"
    )]
    Java {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },

    #[command(
        about = "Build + package Python artifacts",
        long_about = "Build + package Python artifacts.\n\nOutputs:\n  - Python source package: {targets.python.output}\n  - Wheel output:          defaults to {targets.python.output}/wheelhouse\n\nConfig:\n  - [targets.python].module_name overrides the generated package module\n  - [targets.python.wheel].output overrides the wheel directory\n  - [targets.python.wheel].interpreters or repeated --python values select the Python build matrix\n"
    )]
    Python {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,

        #[arg(long, help = "Enable experimental targets/features")]
        experimental: bool,

        #[arg(
            long = "python",
            action = clap::ArgAction::Append,
            value_name = "PYTHON",
            help = "Python interpreter executable or path (repeatable)"
        )]
        python_interpreters: Vec<String>,
    },

    #[command(
        about = "Build + package Dart artifacts",
        long_about = "Build + package Dart artifacts.\n\nOutputs:\n  - Dart package: {targets.dart.output}\n"
    )]
    Dart {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,

        #[arg(long, help = "Enable experimental targets/features")]
        experimental: bool,
    },

    #[command(
        about = "Build + package C# artifacts",
        long_about = "Build + package C# artifacts.\n\nOutputs:\n  - C# package project: {targets.csharp.output}\n  - NuGet package:      {targets.csharp.package_output}\n  - Native assets:      runtimes/<rid>/native inside the .nupkg\n"
    )]
    Csharp {
        #[arg(long)]
        release: bool,

        #[arg(long, default_value = "true")]
        regenerate: bool,

        #[arg(long)]
        no_build: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PackLayoutArg {
    #[value(help = "Single SwiftPM package with wrapper target + generated sources")]
    Bundled,
    #[value(help = "Binary-only SwiftPM package; generate Swift sources outside package")]
    Split,
    #[value(help = "Standalone FFI SwiftPM package (binary target + generated Swift target)")]
    FfiOnly,
}

fn main() {
    let cli = Cli::parse();
    let config_paths = ConfigPaths::from(&cli);

    let verbosity = if cli.quiet {
        reporter::Verbosity::Quiet
    } else if cli.verbose > 0 {
        reporter::Verbosity::Verbose
    } else {
        reporter::Verbosity::Normal
    };

    let reporter = reporter::Reporter::new(verbosity);
    let result = execute_command(cli.command, &reporter, cli.cargo_args, &config_paths);

    if let Err(err) = result {
        eprintln!("\n{} {}", console::style("error:").red().bold(), err);
        std::process::exit(1);
    }
}

fn execute_command(
    command: Commands,
    reporter: &reporter::Reporter,
    cargo_args: Vec<String>,
    config_paths: &ConfigPaths,
) -> Result<()> {
    match command {
        Commands::Init { name } => {
            let options = resolve_init_options(name, config_paths)?;
            run_init(options).map(|_| ())
        }

        Commands::Check {
            fix,
            apple,
            android,
            wasm,
        } => {
            let explicit_target_selected = apple || android || wasm;
            let config = load_config_if_present(config_paths)?;
            let check_wasm = if explicit_target_selected { wasm } else { true };
            let wasm_target_triple = if check_wasm {
                configured_wasm_target_triple_for_diagnostics(config.as_ref())
            } else {
                None
            };
            let options = CheckOptions {
                fix,
                apple: if explicit_target_selected {
                    apple
                } else {
                    true
                },
                apple_targets: configured_apple_targets_for_diagnostics(config.as_ref()),
                android: if explicit_target_selected {
                    android
                } else {
                    true
                },
                android_targets: configured_android_targets_for_diagnostics(config.as_ref()),
                wasm: check_wasm,
                wasm_target_triple,
            };
            run_check(options).map(|_| ())
        }

        Commands::Doctor {
            apple,
            android,
            wasm,
        } => {
            let explicit_target_selected = apple || android || wasm;
            let doctor_config =
                resolve_doctor_config(load_config_if_present(config_paths), config_paths);
            let diagnostic_config = match &doctor_config.summary {
                ConfigSummary::Loaded(config) => Some(config.as_ref()),
                ConfigSummary::Missing | ConfigSummary::Invalid(_) => None,
            };
            let apple_targets = configured_apple_targets_for_diagnostics(diagnostic_config);
            let android_targets = configured_android_targets_for_diagnostics(diagnostic_config);
            let wasm_target_triple =
                configured_wasm_target_triple_for_diagnostics(diagnostic_config);
            let options = DoctorOptions {
                apple: if explicit_target_selected {
                    apple
                } else {
                    true
                },
                apple_targets,
                android: if explicit_target_selected {
                    android
                } else {
                    true
                },
                android_targets,
                wasm: if explicit_target_selected { wasm } else { true },
                wasm_target_triple,
                config_summary: doctor_config.summary,
                config_path: PathBuf::from("boltffi.toml"),
                overlay_path: config_paths.overlay.clone(),
                config_warning: doctor_config.warning,
            };
            run_doctor(options)
        }

        Commands::Generate {
            target,
            output,
            experimental,
        } => {
            let config = load_config(config_paths)?;
            let options = GenerateOptions {
                target: target
                    .map(|t| match t {
                        GenerateTargetArg::Swift => GenerateTarget::Swift,
                        GenerateTargetArg::Kotlin => GenerateTarget::Kotlin,
                        GenerateTargetArg::Java => GenerateTarget::Java,
                        GenerateTargetArg::Header => GenerateTarget::Header,
                        GenerateTargetArg::Typescript => GenerateTarget::Typescript,
                        GenerateTargetArg::Dart => GenerateTarget::Dart,
                        GenerateTargetArg::Python => GenerateTarget::Python,
                        GenerateTargetArg::Csharp => GenerateTarget::CSharp,
                        GenerateTargetArg::All => GenerateTarget::All,
                    })
                    .unwrap_or(GenerateTarget::All),
                output,
                experimental,
            };
            run_generate_with_output(&config, options)
        }

        Commands::Build { platform, release } => {
            let config = load_config(config_paths)?;
            let options = BuildCommandOptions {
                platform: platform
                    .map(|p| match p {
                        BuildPlatformArg::Apple => BuildPlatform::Apple,
                        BuildPlatformArg::Android => BuildPlatform::Android,
                        BuildPlatformArg::Wasm => BuildPlatform::Wasm,
                        BuildPlatformArg::Dart => BuildPlatform::Dart,
                        BuildPlatformArg::All => BuildPlatform::All,
                    })
                    .unwrap_or(BuildPlatform::All),
                release,
                cargo_args,
            };
            run_build(&config, options).map(|_| ())
        }

        Commands::Pack { target } => {
            let config = load_config(config_paths)?;
            let command = match target {
                PackTargetArg::All {
                    release,
                    regenerate,
                    no_build,
                    experimental,
                    python_interpreters,
                } => PackCommand::All(PackAllOptions {
                    execution: pack_execution_options(
                        release,
                        regenerate,
                        no_build,
                        cargo_args.clone(),
                    ),
                    experimental,
                    python_interpreters,
                }),
                PackTargetArg::Apple {
                    release,
                    version,
                    regenerate,
                    no_build,
                    spm_only,
                    xcframework_only,
                    layout,
                } => PackCommand::Apple(PackAppleOptions {
                    execution: pack_execution_options(
                        release,
                        regenerate,
                        no_build,
                        cargo_args.clone(),
                    ),
                    version,
                    spm_only,
                    xcframework_only,
                    layout: layout.map(|l| match l {
                        PackLayoutArg::Bundled => crate::config::SpmLayout::Bundled,
                        PackLayoutArg::Split => crate::config::SpmLayout::Split,
                        PackLayoutArg::FfiOnly => crate::config::SpmLayout::FfiOnly,
                    }),
                }),
                PackTargetArg::Android {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::Android(PackAndroidOptions {
                    execution: pack_execution_options(
                        release,
                        regenerate,
                        no_build,
                        cargo_args.clone(),
                    ),
                }),
                PackTargetArg::Wasm {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::Wasm(PackWasmOptions {
                    execution: pack_execution_options(
                        release,
                        regenerate,
                        no_build,
                        cargo_args.clone(),
                    ),
                }),
                PackTargetArg::Java {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::Java(PackJavaOptions {
                    execution: pack_execution_options(
                        release,
                        regenerate,
                        no_build,
                        cargo_args.clone(),
                    ),
                    experimental: false,
                }),
                PackTargetArg::Python {
                    release,
                    regenerate,
                    no_build,
                    experimental,
                    python_interpreters,
                } => PackCommand::Python(PackPythonOptions {
                    execution: pack_execution_options(
                        release,
                        regenerate,
                        no_build,
                        cargo_args.clone(),
                    ),
                    experimental,
                    python_interpreters,
                }),
                PackTargetArg::Dart {
                    release,
                    regenerate,
                    no_build,
                    experimental,
                } => PackCommand::Dart(PackDartOptions {
                    execution: pack_execution_options(release, regenerate, no_build, cargo_args),
                    experimental,
                }),
                PackTargetArg::Csharp {
                    release,
                    regenerate,
                    no_build,
                } => PackCommand::CSharp(PackCSharpOptions {
                    execution: pack_execution_options(release, regenerate, no_build, cargo_args),
                }),
            };
            run_pack(&config, command, reporter)
        }

        Commands::Release { platform } => {
            let config = load_config(config_paths)?;
            run_release(&config, platform, reporter, cargo_args)
        }

        Commands::Verify { path, json } => {
            let options = VerifyOptions { path, json };
            run_verify(options).map(|verified| {
                if !verified {
                    std::process::exit(1);
                }
            })
        }
    }
}

fn pack_execution_options(
    release: bool,
    regenerate: bool,
    no_build: bool,
    cargo_args: Vec<String>,
) -> PackExecutionOptions {
    PackExecutionOptions {
        release,
        regenerate,
        no_build,
        cargo_args,
    }
}

#[derive(Debug, Clone)]
struct ConfigPaths {
    overlay: Option<PathBuf>,
}

impl From<&Cli> for ConfigPaths {
    fn from(cli: &Cli) -> Self {
        Self {
            overlay: cli.overlay.clone(),
        }
    }
}

impl fmt::Display for ConfigPaths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.overlay {
            Some(overlay) => write!(f, "boltffi.toml with overlay {}", overlay.display()),
            None => f.write_str("boltffi.toml"),
        }
    }
}

fn load_config(config_paths: &ConfigPaths) -> Result<Config> {
    let config_path = PathBuf::from("boltffi.toml");

    if !config_path.exists() {
        return Err(CliError::ConfigNotFound(config_path));
    }

    match config_paths.overlay.as_deref() {
        Some(overlay_path) => Config::load_with_overlay(&config_path, Some(overlay_path)),
        None => Config::load(&config_path),
    }
    .map_err(Into::into)
}

fn load_config_if_present(config_paths: &ConfigPaths) -> Result<Option<Config>> {
    let config_path = PathBuf::from("boltffi.toml");

    if !config_path.exists() {
        if config_paths.overlay.is_some() {
            return Err(CliError::ConfigNotFound(config_path));
        }
        return Ok(None);
    }

    match config_paths.overlay.as_deref() {
        Some(overlay_path) => Config::load_with_overlay(&config_path, Some(overlay_path)),
        None => Config::load(&config_path),
    }
    .map(Some)
    .map_err(Into::into)
}

fn configured_apple_targets_for_diagnostics(
    config: Option<&Config>,
) -> Vec<crate::target::RustTarget> {
    config
        .filter(|config| config.is_apple_enabled())
        .map(|config| config.apple_targets())
        .unwrap_or_else(|| crate::target::RustTarget::ALL_IOS.to_vec())
}

fn configured_android_targets_for_diagnostics(
    config: Option<&Config>,
) -> Vec<crate::target::RustTarget> {
    config
        .filter(|config| config.is_android_enabled())
        .map(|config| config.android_targets())
        .unwrap_or_else(|| crate::target::RustTarget::ALL_ANDROID.to_vec())
}

fn configured_wasm_target_triple_for_diagnostics(config: Option<&Config>) -> Option<String> {
    config.map(|config| config.wasm_triple().to_string())
}

struct DoctorConfig {
    summary: ConfigSummary,
    warning: Option<String>,
}

fn resolve_init_options(name: Option<String>, config_paths: &ConfigPaths) -> Result<InitOptions> {
    if config_paths.overlay.is_some() {
        return Err(CliError::CommandFailed {
            command: "--overlay is not supported with init".to_string(),
            status: None,
        });
    }

    Ok(InitOptions {
        name,
        path: std::env::current_dir().unwrap_or_default(),
    })
}

fn resolve_doctor_config(
    config_result: Result<Option<Config>>,
    config_paths: &ConfigPaths,
) -> DoctorConfig {
    match config_result {
        Ok(Some(config)) => DoctorConfig {
            summary: ConfigSummary::Loaded(Box::new(config)),
            warning: None,
        },
        Ok(None) => DoctorConfig {
            summary: ConfigSummary::Missing,
            warning: None,
        },
        Err(CliError::ConfigNotFound(_)) => DoctorConfig {
            summary: ConfigSummary::Missing,
            warning: Some(format!(
                "failed to load config {} ({}); using default Apple/Android/WASM target checks",
                config_paths,
                CliError::ConfigNotFound(PathBuf::from("boltffi.toml"))
            )),
        },
        Err(error) => DoctorConfig {
            summary: ConfigSummary::Invalid(error.to_string()),
            warning: Some(format!(
                "failed to load config {} ({}); using default Apple/Android/WASM target checks",
                config_paths, error
            )),
        },
    }
}

fn run_release(
    config: &Config,
    platform: Option<BuildPlatformArg>,
    reporter: &reporter::Reporter,
    cargo_args: Vec<String>,
) -> Result<()> {
    reporter.section("🚀", "Running full release pipeline");

    let check_options = CheckOptions {
        fix: false,
        apple: config.is_apple_enabled(),
        apple_targets: config.apple_targets(),
        android: config.is_android_enabled(),
        android_targets: config.android_targets(),
        wasm: config.is_wasm_enabled(),
        wasm_target_triple: Some(config.wasm_triple().to_string()),
    };

    println!("[1/4] Checking environment...");
    let env_ok = run_check(check_options)?;

    if !env_ok {
        println!("Environment check failed. Run 'boltffi check --fix' to install missing targets.");
        return Ok(());
    }

    if release_requires_java_environment_validation(config, platform)
        && let Err(error) = check_java_packaging_prereqs(config, true, &cargo_args)
    {
        println!("JVM packaging preflight failed: {error}");
        return Err(error);
    }
    println!();

    println!("[2/4] Building...");
    let build_options = BuildCommandOptions {
        platform: platform
            .map(|p| match p {
                BuildPlatformArg::Apple => BuildPlatform::Apple,
                BuildPlatformArg::Android => BuildPlatform::Android,
                BuildPlatformArg::Wasm => BuildPlatform::Wasm,
                BuildPlatformArg::Dart => BuildPlatform::Dart,
                BuildPlatformArg::All => BuildPlatform::All,
            })
            .unwrap_or(BuildPlatform::All),
        release: true,
        cargo_args: cargo_args.clone(),
    };
    run_build(config, build_options)?;
    println!();

    println!("[3/4] Generating bindings...");
    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::All,
            output: None,
            experimental: false,
        },
    )?;
    println!();

    println!("[4/4] Packaging...");

    for command in release_pack_commands(config, platform, &cargo_args) {
        run_pack(config, command, reporter)?;
    }

    println!();
    println!("Release complete!");

    Ok(())
}

fn release_pack_commands(
    config: &Config,
    platform: Option<BuildPlatformArg>,
    cargo_args: &[String],
) -> Vec<PackCommand> {
    let mut commands = Vec::new();

    match platform {
        Some(BuildPlatformArg::Apple) => {
            if config.is_apple_enabled() {
                commands.push(PackCommand::Apple(PackAppleOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                    version: None,
                    spm_only: false,
                    xcframework_only: false,
                    layout: None,
                }));
            }
        }
        Some(BuildPlatformArg::Android) => {
            if config.is_android_enabled() {
                commands.push(PackCommand::Android(PackAndroidOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                }));
            }
        }
        Some(BuildPlatformArg::Wasm) => {
            if config.is_wasm_enabled() {
                commands.push(PackCommand::Wasm(PackWasmOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                }));
            }
        }
        Some(BuildPlatformArg::Dart) => {
            if config.is_dart_enabled() {
                commands.push(PackCommand::Dart(PackDartOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                    experimental: true,
                }));
            }
        }
        Some(BuildPlatformArg::All) | None => {
            if config.is_apple_enabled() {
                commands.push(PackCommand::Apple(PackAppleOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                    version: None,
                    spm_only: false,
                    xcframework_only: false,
                    layout: None,
                }));
            }
            if config.is_android_enabled() {
                commands.push(PackCommand::Android(PackAndroidOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                }));
            }
            if config.is_wasm_enabled() {
                commands.push(PackCommand::Wasm(PackWasmOptions {
                    execution: pack_execution_options(true, false, true, cargo_args.to_vec()),
                }));
            }
            if config.should_process(Target::Python, false) {
                commands.push(PackCommand::Python(PackPythonOptions {
                    execution: pack_execution_options(true, false, false, cargo_args.to_vec()),
                    experimental: false,
                    python_interpreters: Vec::new(),
                }));
            }
            if config.should_process(Target::Java, false) {
                commands.push(PackCommand::Java(PackJavaOptions {
                    execution: pack_execution_options(true, true, false, cargo_args.to_vec()),
                    experimental: false,
                }));
            }

            if config.should_process(Target::Dart, false) {
                commands.push(PackCommand::Dart(PackDartOptions {
                    execution: pack_execution_options(true, false, false, cargo_args.to_vec()),
                    experimental: false,
                }));
            }

            if config.is_csharp_enabled() {
                commands.push(PackCommand::CSharp(PackCSharpOptions {
                    execution: pack_execution_options(true, true, false, cargo_args.to_vec()),
                }));
            }
        }
    }

    commands
}

fn release_requires_java_environment_validation(
    config: &Config,
    platform: Option<BuildPlatformArg>,
) -> bool {
    matches!(platform, Some(BuildPlatformArg::All) | None)
        && config.should_process(Target::Java, false)
}

#[cfg(test)]
mod tests {
    use super::{
        BuildPlatformArg, Cli, Commands, ConfigPaths, GenerateTargetArg, PackTargetArg,
        configured_android_targets_for_diagnostics, configured_apple_targets_for_diagnostics,
        configured_wasm_target_triple_for_diagnostics, load_config_if_present,
        release_pack_commands, release_requires_java_environment_validation, resolve_doctor_config,
        resolve_init_options,
    };
    use crate::commands::doctor::ConfigSummary;
    use crate::commands::pack::PackCommand;
    use crate::target::RustTarget;
    use crate::{cli::CliError, config::Config};
    use clap::Parser;
    use std::path::PathBuf;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    #[test]
    fn diagnostics_ignore_disabled_apple_target_configuration() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
enabled = false
include_macos = true
ios_architectures = ["arm64"]
simulator_architectures = ["arm64"]
macos_architectures = ["arm64"]
"#,
        );

        assert_eq!(
            configured_apple_targets_for_diagnostics(Some(&config)),
            RustTarget::ALL_IOS.to_vec()
        );
    }

    #[test]
    fn diagnostics_ignore_disabled_android_target_configuration() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.android]
enabled = false
architectures = ["arm64"]
"#,
        );

        assert_eq!(
            configured_android_targets_for_diagnostics(Some(&config)),
            RustTarget::ALL_ANDROID.to_vec()
        );
    }

    #[test]
    fn diagnostics_preserve_wasm_triple_when_target_is_disabled() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.wasm]
enabled = false
triple = "wasm32-wasip1"
"#,
        );

        assert_eq!(
            configured_wasm_target_triple_for_diagnostics(Some(&config)),
            Some("wasm32-wasip1".to_string())
        );
    }

    #[test]
    fn doctor_falls_back_to_defaults_when_config_load_fails() {
        let config_paths = ConfigPaths {
            overlay: Some(PathBuf::from("boltffi.ci.toml")),
        };
        let resolved = resolve_doctor_config(
            Err(crate::config::ConfigError::Validation("bad config".to_string()).into()),
            &config_paths,
        );

        assert!(matches!(resolved.summary, ConfigSummary::Invalid(_)));
        assert!(resolved.warning.as_deref().is_some_and(|warning| {
            warning.contains("using default Apple/Android/WASM target checks")
        }));
    }

    #[test]
    fn missing_base_config_fails_when_overlay_is_requested() {
        let config_paths = ConfigPaths {
            overlay: Some(PathBuf::from("boltffi.ci.toml")),
        };

        let error =
            load_config_if_present(&config_paths).expect_err("overlay without base should fail");

        assert!(
            matches!(error, CliError::ConfigNotFound(path) if path == std::path::Path::new("boltffi.toml"))
        );
    }

    #[test]
    fn doctor_warns_when_overlay_is_requested_without_base_config() {
        let config_paths = ConfigPaths {
            overlay: Some(PathBuf::from("boltffi.ci.toml")),
        };
        let resolved = resolve_doctor_config(
            Err(CliError::ConfigNotFound(PathBuf::from("boltffi.toml"))),
            &config_paths,
        );

        assert!(matches!(resolved.summary, ConfigSummary::Missing));
        assert!(resolved.warning.as_deref().is_some_and(|warning| {
            warning.contains("boltffi.toml with overlay boltffi.ci.toml")
                && warning.contains("config file not found")
        }));
    }

    #[test]
    fn init_uses_current_directory() {
        let config_paths = ConfigPaths { overlay: None };

        let options = resolve_init_options(Some("mylib".to_string()), &config_paths)
            .expect("init options should resolve");

        assert_eq!(options.path, std::env::current_dir().unwrap_or_default());
    }

    #[test]
    fn init_rejects_overlay_config() {
        let config_paths = ConfigPaths {
            overlay: Some(PathBuf::from("boltffi.ci.toml")),
        };

        let error = match resolve_init_options(None, &config_paths) {
            Ok(_) => panic!("overlay should fail"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "--overlay is not supported with init"
        ));
    }

    #[test]
    fn release_all_includes_java_packaging_without_no_build() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
enabled = true

[targets.android]
enabled = true

[targets.wasm]
enabled = true

[targets.java.jvm]
enabled = true
"#,
        );

        let commands = release_pack_commands(&config, Some(BuildPlatformArg::All), &[]);

        assert_eq!(commands.len(), 4);
        assert!(matches!(
            &commands[0],
            PackCommand::Apple(options) if options.execution.no_build
        ));
        assert!(matches!(
            &commands[1],
            PackCommand::Android(options) if options.execution.no_build
        ));
        assert!(matches!(
            &commands[2],
            PackCommand::Wasm(options) if options.execution.no_build
        ));
        assert!(matches!(
            &commands[3],
            PackCommand::Java(options)
                if !options.execution.no_build
                    && options.execution.release
                    && options.execution.regenerate
                    && !options.experimental
        ));
    }

    #[test]
    fn release_platform_filter_does_not_add_java_for_non_all_platforms() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.apple]
enabled = true

[targets.java.jvm]
enabled = true
"#,
        );

        let commands = release_pack_commands(&config, Some(BuildPlatformArg::Apple), &[]);

        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            PackCommand::Apple(options) if options.execution.no_build
        ));
    }

    #[test]
    fn release_all_includes_java_when_enabled() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
"#,
        );

        let commands = release_pack_commands(&config, Some(BuildPlatformArg::All), &[]);

        assert!(
            commands
                .iter()
                .any(|command| matches!(command, PackCommand::Java(_)))
        );
        assert!(release_requires_java_environment_validation(
            &config,
            Some(BuildPlatformArg::All)
        ));
    }

    #[test]
    fn release_all_includes_python_packaging_when_enabled_and_experimental() {
        let config = parse_config(
            r#"
experimental = ["python"]

[package]
name = "mylib"

[targets.python]
enabled = true
"#,
        );

        let commands = release_pack_commands(&config, Some(BuildPlatformArg::All), &[]);

        assert!(commands.iter().any(|command| matches!(
            command,
            PackCommand::Python(options)
                if options.execution.release
                    && !options.execution.regenerate
                    && !options.execution.no_build
                    && !options.experimental
        )));
    }

    #[test]
    fn release_all_includes_csharp_packaging_when_enabled() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.csharp]
enabled = true
"#,
        );

        let commands = release_pack_commands(&config, Some(BuildPlatformArg::All), &[]);

        assert!(commands.iter().any(|command| matches!(
            command,
            PackCommand::CSharp(options)
                if options.execution.release
                    && options.execution.regenerate
                    && !options.execution.no_build
        )));
    }

    #[test]
    fn cli_parses_generate_python_target() {
        let cli = Cli::try_parse_from(["boltffi", "generate", "python", "--experimental"])
            .expect("cli parse should succeed");

        assert!(matches!(
            cli.command,
            Commands::Generate {
                target: Some(GenerateTargetArg::Python),
                experimental: true,
                ..
            }
        ));
    }

    #[test]
    fn cli_parses_generate_csharp_target_without_experimental_flag() {
        let cli = Cli::try_parse_from(["boltffi", "generate", "csharp"])
            .expect("cli parse should succeed");

        assert!(matches!(
            cli.command,
            Commands::Generate {
                target: Some(GenerateTargetArg::Csharp),
                experimental: false,
                ..
            }
        ));
    }

    #[test]
    fn cli_parses_pack_python_target() {
        let cli = Cli::try_parse_from(["boltffi", "pack", "python", "--experimental"])
            .expect("cli parse should succeed");

        assert!(matches!(
            cli.command,
            Commands::Pack {
                target: PackTargetArg::Python {
                    experimental: true,
                    ..
                }
            }
        ));
    }

    #[test]
    fn cli_parses_pack_csharp_target() {
        let cli =
            Cli::try_parse_from(["boltffi", "pack", "csharp"]).expect("cli parse should succeed");

        assert!(matches!(
            cli.command,
            Commands::Pack {
                target: PackTargetArg::Csharp { .. }
            }
        ));
    }

    #[test]
    fn cli_parses_pack_python_interpreter_matrix() {
        let cli = Cli::try_parse_from([
            "boltffi",
            "pack",
            "python",
            "--experimental",
            "--python",
            "python3.12",
            "--python",
            "python3.13",
        ])
        .expect("cli parse should succeed");

        assert!(matches!(
            cli.command,
            Commands::Pack {
                target: PackTargetArg::Python {
                    python_interpreters,
                    ..
                }
            } if python_interpreters == vec!["python3.12".to_string(), "python3.13".to_string()]
        ));
    }

    #[test]
    fn release_all_requires_java_environment_validation_when_enabled() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
"#,
        );

        assert!(release_requires_java_environment_validation(
            &config,
            Some(BuildPlatformArg::All)
        ));
        assert!(release_requires_java_environment_validation(&config, None));
    }

    #[test]
    fn release_platform_filter_skips_java_environment_validation_for_non_all_platforms() {
        let config = parse_config(
            r#"
[package]
name = "mylib"

[targets.java.jvm]
enabled = true
"#,
        );

        assert!(!release_requires_java_environment_validation(
            &config,
            Some(BuildPlatformArg::Apple)
        ));
        assert!(!release_requires_java_environment_validation(
            &config,
            Some(BuildPlatformArg::Android)
        ));
        assert!(!release_requires_java_environment_validation(
            &config,
            Some(BuildPlatformArg::Wasm)
        ));
    }
}
