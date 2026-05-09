use crate::cli::Result;
use crate::config::{Config, Target};
use crate::reporter::Reporter;

use super::{
    PackAllOptions, PackAndroidOptions, PackAppleOptions, PackCSharpOptions, PackDartOptions,
    PackJavaOptions, PackPythonOptions, PackWasmOptions, pack_android, pack_apple, pack_csharp,
    pack_dart, pack_java, pack_python, pack_wasm, prepare_java_packaging,
};

pub(super) fn pack_all(
    config: &Config,
    options: PackAllOptions,
    reporter: &Reporter,
) -> Result<()> {
    super::ensure_java_no_build_supported(
        config,
        options.execution.no_build,
        options.experimental,
        "pack all",
    )?;
    let prepared_java_packaging = config
        .should_process(Target::Java, options.experimental)
        .then(|| {
            prepare_java_packaging(
                config,
                options.execution.release,
                &options.execution.cargo_args,
            )
        })
        .transpose()?;

    let mut packed_any = false;

    if config.is_apple_enabled() {
        pack_apple(
            config,
            PackAppleOptions {
                execution: options.execution.clone(),
                version: None,
                spm_only: false,
                xcframework_only: false,
                layout: None,
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_android_enabled() {
        pack_android(
            config,
            PackAndroidOptions {
                execution: options.execution.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_wasm_enabled() {
        pack_wasm(
            config,
            PackWasmOptions {
                execution: options.execution.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.should_process(Target::Java, options.experimental) {
        pack_java(
            config,
            PackJavaOptions {
                execution: options.execution.clone(),
                experimental: options.experimental,
            },
            prepared_java_packaging,
            reporter,
        )?;
        packed_any = true;
    }

    if config.should_process(Target::Python, options.experimental) {
        pack_python(
            config,
            PackPythonOptions {
                execution: options.execution.clone(),
                experimental: options.experimental,
                python_interpreters: options.python_interpreters.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.should_process(Target::Dart, options.experimental) {
        pack_dart(
            config,
            PackDartOptions {
                execution: options.execution.clone(),
                experimental: options.experimental,
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_csharp_enabled() {
        pack_csharp(
            config,
            PackCSharpOptions {
                execution: options.execution,
            },
            reporter,
        )?;
        packed_any = true;
    }

    if !packed_any {
        reporter.warning("no targets enabled in config");
    }

    reporter.finish();
    Ok(())
}
