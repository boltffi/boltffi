use crate::cli::Result;
use crate::config::{Config, Target};
use crate::reporter::Reporter;

use super::{
    PackAllOptions, PackAndroidOptions, PackAppleOptions, PackJavaOptions, PackPythonOptions,
    PackWasmOptions, pack_android, pack_apple, pack_java, pack_python, pack_wasm,
    prepare_java_packaging,
};

pub(super) fn pack_all(
    config: &Config,
    options: PackAllOptions,
    reporter: &Reporter,
) -> Result<()> {
    super::ensure_java_no_build_supported(
        config,
        options.no_build,
        options.experimental,
        "pack all",
    )?;
    let prepared_java_packaging = config
        .should_process(Target::Java, options.experimental)
        .then(|| {
            prepare_java_packaging(
                config,
                options.release,
                &options.cargo_args,
                options.cargo_build_cmd.as_deref(),
            )
        })
        .transpose()?;

    let mut packed_any = false;

    if config.is_apple_enabled() {
        pack_apple(
            config,
            PackAppleOptions {
                release: options.release,
                version: None,
                regenerate: options.regenerate,
                no_build: options.no_build,
                spm_only: false,
                xcframework_only: false,
                layout: None,
                cargo_args: options.cargo_args.clone(),
                cargo_build_cmd: options.cargo_build_cmd.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_android_enabled() {
        pack_android(
            config,
            PackAndroidOptions {
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                cargo_args: options.cargo_args.clone(),
                cargo_build_cmd: options.cargo_build_cmd.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.is_wasm_enabled() {
        pack_wasm(
            config,
            PackWasmOptions {
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                cargo_args: options.cargo_args.clone(),
                cargo_build_cmd: options.cargo_build_cmd.clone(),
            },
            reporter,
        )?;
        packed_any = true;
    }

    if config.should_process(Target::Java, options.experimental) {
        pack_java(
            config,
            PackJavaOptions {
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                experimental: options.experimental,
                cargo_args: options.cargo_args.clone(),
                cargo_build_cmd: options.cargo_build_cmd.clone(),
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
                release: options.release,
                regenerate: options.regenerate,
                no_build: options.no_build,
                experimental: options.experimental,
                python_interpreters: options.python_interpreters.clone(),
                cargo_args: options.cargo_args.clone(),
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
