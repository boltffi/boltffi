use std::path::Path;

use crate::{
    build::{
        BuildOptions, BuildSelection, Builder, CargoBuildProfile, OutputCallback, all_successful,
        failed_targets, resolve_build_profile,
    },
    cargo::Cargo,
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::PackDartOptions,
    },
    config::Config,
    pack::{
        PackError,
        dart_web::{generate_and_vendor_web, pack_wrapped_wasm_module},
        print_cargo_line, resolve_build_cargo_args,
    },
    reporter::{Reporter, Step},
};

fn build_dart_targets(
    config: &Config,
    release: bool,
    build_cargo_args: &[String],
    step: &Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| print_cargo_line(line)))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        selection: BuildSelection::Package {
            package: config.library_name().to_string(),
            cargo_args: build_cargo_args.to_vec(),
        },
        on_output,
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_targets(&config.dart_targets())?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(CliError::Pack(PackError::BuildFailed { targets: failed }))
}

pub(crate) fn pack_dart(
    config: &Config,
    options: PackDartOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_dart_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.dart.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("☕", "Packing Dart");

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile = resolve_build_profile(options.execution.release, &build_cargo_args);

    if !options.execution.no_build {
        let step = reporter.step("Building Rust cdylib");
        build_dart_targets(
            config,
            matches!(build_profile, CargoBuildProfile::Release),
            &build_cargo_args,
            &step,
        )?;
        step.finish_success();
    }

    if options.execution.regenerate {
        let step = reporter.step("Generating Dart bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Dart,
                output: Some(config.dart_output()),
                experimental: options.experimental,
                cargo_args: build_cargo_args.clone(),
                deny_skipped: options.execution.deny_skipped,
            },
        )?;

        step.finish_success();
    }

    let step = reporter.step("Packaging native libraries");

    let cargo = Cargo::current(&build_cargo_args)?;

    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let package_selector =
        cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);

    let libraries = metadata.resolve_built_libraries_for_targets(
        &cargo_manifest_path,
        build_profile.output_directory_name(),
        &config.crate_artifact_name(),
        package_selector.as_deref(),
        &config.dart_targets(),
    )?;

    let package_dir = config.dart_output().join(&config.package.name);
    let native_libs_dir = package_dir.join("native");
    std::fs::create_dir_all(&native_libs_dir).map_err(|source| {
        CliError::CreateDirectoryFailed {
            path: native_libs_dir.clone(),
            source,
        }
    })?;

    for l in libraries {
        let native_lib_triple_dir = native_libs_dir.join(l.target.triple());
        std::fs::create_dir_all(&native_lib_triple_dir).map_err(|source| {
            CliError::CreateDirectoryFailed {
                path: native_lib_triple_dir.clone(),
                source,
            }
        })?;

        let native_lib_filepath =
            native_lib_triple_dir.join(l.path.file_name().expect("file shouldn't terminate in .."));

        std::fs::copy(&l.path, &native_lib_filepath).map_err(|source| CliError::CopyFailed {
            from: l.path,
            to: native_lib_filepath,
            source,
        })?;
    }

    step.finish_success();

    if config.is_dart_web_enabled() {
        unify_native_and_web(config, &options, &package_dir, reporter)?;
    }

    reporter.finish();
    Ok(())
}

// Folds the dart_web output into this same package: native goes under
// src/native, web under src/web, picked by Dart's own
// dart.library.js_interop conditional export.
fn unify_native_and_web(
    config: &Config,
    options: &PackDartOptions,
    package_dir: &Path,
    reporter: &Reporter,
) -> Result<()> {
    reporter.section("🔗", "Unifying native + web Dart packages");

    let package_name = &config.package.name;
    let lib_dir = package_dir.join("lib");
    let native_dir = lib_dir.join("src/native");
    let web_dir = lib_dir.join("src/web");

    {
        let step = reporter.step("Moving native bindings under src/native");
        std::fs::create_dir_all(&native_dir).map_err(|source| CliError::CreateDirectoryFailed {
            path: native_dir.clone(),
            source,
        })?;
        let native_file = lib_dir.join(format!("{package_name}.dart"));
        let native_dest = native_dir.join(format!("{package_name}.dart"));
        std::fs::rename(&native_file, &native_dest).map_err(|source| CliError::CopyFailed {
            from: native_file,
            to: native_dest,
            source,
        })?;
        step.finish_success();
    }

    pack_wrapped_wasm_module(config, &options.execution, reporter)?;
    generate_and_vendor_web(
        config,
        &options.execution,
        options.experimental,
        &web_dir,
        reporter,
    )?;

    {
        let step = reporter.step("Writing the conditional-export shim");
        let shim_path = lib_dir.join(format!("{package_name}.dart"));
        let shim = format!(
            "library;\n\n\
             export 'src/native/{package_name}.dart'\n\
             \x20\x20\x20\x20if (dart.library.js_interop) 'src/web/{package_name}.dart';\n"
        );
        std::fs::write(&shim_path, shim).map_err(|source| CliError::WriteFailed {
            path: shim_path,
            source,
        })?;
        step.finish_success();
    }

    {
        let step = reporter.step("Writing web setup instructions");
        write_web_setup_doc(package_dir, package_name)?;
        step.finish_success();
    }

    Ok(())
}

fn write_web_setup_doc(package_dir: &Path, package_name: &str) -> Result<()> {
    let js_namespace = format!("__boltffi_{package_name}");
    let doc = format!(
        "# Web setup\n\n\
         The contents of `lib/src/web/` have to be copied into your app's `web/`\n\
         directory (Flutter or plain Dart web) once. They won't be picked up\n\
         automatically just by depending on this package.\n\n\
         ## 1. Copy these files, from `lib/src/web/`\n\n\
         - `{package_name}_web_loader.mjs`\n\
         - `web/` (whole folder — compiled JS, the wasm binary, and the vendored runtime)\n\n\
         No npm install, no build step — everything here resolves via plain relative\n\
         imports a browser understands natively.\n\n\
         ## 2. Add this to `web/index.html`, before your compiled app's own script tag\n\n\
         ```html\n\
         <script type=\"module\" src=\"{package_name}_web_loader.mjs\"></script>\n\
         ```\n\n\
         ## 3. Call this once before using the package\n\n\
         ```dart\n\
         import 'package:{package_name}/{package_name}.dart';\n\n\
         await init();\n\
         ```\n\n\
         `init()` only *waits* for the module the script tag above already started\n\
         loading — it doesn't load anything itself. On native (dart:ffi) targets,\n\
         `init()` isn't part of the generated API at all; only the web half needs it.\n\n\
         ---\n\n\
         Why the manual copy: `pack dart` only ever runs in this package's own repo,\n\
         never in a consuming app's build — there's no hook it could use to place\n\
         files into an app it doesn't know about. And Flutter's own asset-bundling\n\
         system (`flutter: assets:`) isn't a safe substitute here: it serves files\n\
         through a different pipeline that doesn't guarantee the correct MIME types\n\
         for `.wasm`/`.js`, which can silently break loading in production.\n\n\
         The global JS namespace this package's web half uses is `{js_namespace}` —\n\
         only relevant if you're debugging, never something you need to reference.\n"
    );
    let doc_path = package_dir.join("WEB_SETUP.md");
    std::fs::write(&doc_path, doc).map_err(|source| CliError::WriteFailed {
        path: doc_path,
        source,
    })
}
