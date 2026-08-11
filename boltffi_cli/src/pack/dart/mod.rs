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
        move_native_bindings(&lib_dir, &native_dir, package_name)?;
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
    let web_module_name = config.dart_web_module_name();

    {
        let step = reporter.step("Writing the conditional-export shim");
        let shim_path = lib_dir.join(format!("{package_name}.dart"));
        std::fs::write(
            &shim_path,
            conditional_export_shim(package_name, &web_module_name),
        )
        .map_err(|source| CliError::WriteFailed {
            path: shim_path,
            source,
        })?;
        step.finish_success();
    }

    {
        let step = reporter.step("Writing web setup instructions");
        write_web_setup_doc(package_dir, package_name, &web_module_name)?;
        step.finish_success();
    }

    Ok(())
}

// Two cases must be a no-op rather than an error:
// - A second `pack dart` with `--regenerate=false` leaves `lib/<package>.dart`
//   as the conditional-export shim this same function wrote last time, not
//   fresh native bindings -- moving it would clobber the real native
//   bindings already sitting in native_dir.
// - A prior `unify_native_and_web` run already moved the file here and then
//   failed later (e.g. the wasm/web half failed to build) -- on retry,
//   `lib/<package>.dart` no longer exists at all, but native_dir already
//   has it; treating a missing source as an error would make that failure
//   unrecoverable without deleting native_dir by hand.
fn move_native_bindings(lib_dir: &Path, native_dir: &Path, package_name: &str) -> Result<()> {
    std::fs::create_dir_all(native_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: native_dir.to_path_buf(),
        source,
    })?;
    let native_file = lib_dir.join(format!("{package_name}.dart"));
    let native_dest = native_dir.join(format!("{package_name}.dart"));
    if !native_file.exists() && native_dest.exists() {
        return Ok(());
    }
    let existing =
        std::fs::read_to_string(&native_file).map_err(|source| CliError::ReadFailed {
            path: native_file.clone(),
            source,
        })?;
    if existing.starts_with("library;\n\nexport 'src/native/") {
        return Ok(());
    }
    std::fs::rename(&native_file, &native_dest).map_err(|source| CliError::CopyFailed {
        from: native_file,
        to: native_dest,
        source,
    })
}

fn conditional_export_shim(package_name: &str, web_module_name: &str) -> String {
    format!(
        "library;\n\n\
         export 'src/native/{package_name}.dart'\n\
         \x20\x20\x20\x20if (dart.library.js_interop) 'src/web/{web_module_name}.dart';\n"
    )
}

fn write_web_setup_doc(
    package_dir: &Path,
    package_name: &str,
    web_module_name: &str,
) -> Result<()> {
    let js_namespace = format!("__boltffi_{web_module_name}");
    let doc = format!(
        "# Web setup\n\n\
         The contents of `lib/src/web/` have to be copied into your app's `web/`\n\
         directory (Flutter or plain Dart web) once. They won't be picked up\n\
         automatically just by depending on this package.\n\n\
         ## 1. Copy these files, from `lib/src/web/`\n\n\
         - `{web_module_name}_web_loader.mjs`\n\
         - `web/` (whole folder — compiled JS, the wasm binary, and the vendored runtime)\n\n\
         No npm install, no build step — everything here resolves via plain relative\n\
         imports a browser understands natively.\n\n\
         ## 2. Add this to `web/index.html`, before your compiled app's own script tag\n\n\
         ```html\n\
         <script type=\"module\" src=\"{web_module_name}_web_loader.mjs\"></script>\n\
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{conditional_export_shim, move_native_bindings, write_web_setup_doc};

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    #[test]
    fn conditional_export_shim_picks_web_only_under_js_interop() {
        let shim = conditional_export_shim("demo", "demo");

        assert_eq!(
            shim,
            "library;\n\n\
             export 'src/native/demo.dart'\n\
             \x20\x20\x20\x20if (dart.library.js_interop) 'src/web/demo.dart';\n"
        );
    }

    #[test]
    fn conditional_export_shim_references_a_custom_web_module_name() {
        let shim = conditional_export_shim("demo", "demo_web");

        assert!(shim.contains("if (dart.library.js_interop) 'src/web/demo_web.dart';"));
    }

    #[test]
    fn move_native_bindings_relocates_the_generated_file() {
        let package_dir = unique_temp_dir("boltffi-move-native-bindings-test");
        let lib_dir = package_dir.join("lib");
        let native_dir = lib_dir.join("src/native");
        std::fs::create_dir_all(&lib_dir).expect("create lib dir");
        std::fs::write(lib_dir.join("demo.dart"), "library demo;\n").expect("write native file");

        move_native_bindings(&lib_dir, &native_dir, "demo").expect("move succeeds");

        assert!(!lib_dir.join("demo.dart").exists());
        let moved = std::fs::read_to_string(native_dir.join("demo.dart")).expect("moved file");
        assert_eq!(moved, "library demo;\n");

        std::fs::remove_dir_all(&package_dir).expect("cleanup temp dir");
    }

    #[test]
    fn move_native_bindings_fails_when_the_native_file_is_missing() {
        let package_dir = unique_temp_dir("boltffi-move-native-bindings-missing-test");
        let lib_dir = package_dir.join("lib");
        let native_dir = lib_dir.join("src/native");
        std::fs::create_dir_all(&lib_dir).expect("create lib dir");

        let result = move_native_bindings(&lib_dir, &native_dir, "demo");

        assert!(result.is_err());
        std::fs::remove_dir_all(&package_dir).expect("cleanup temp dir");
    }

    /// A second `pack dart --regenerate=false` must not clobber the real
    /// native bindings already sitting in native_dir with the shim
    /// `lib/<package>.dart` was rewritten into on the previous run.
    #[test]
    fn move_native_bindings_is_a_no_op_when_the_lib_file_is_already_the_shim() {
        let package_dir = unique_temp_dir("boltffi-move-native-bindings-idempotent-test");
        let lib_dir = package_dir.join("lib");
        let native_dir = lib_dir.join("src/native");
        std::fs::create_dir_all(&native_dir).expect("create native dir");
        std::fs::write(
            native_dir.join("demo.dart"),
            "library demo;\n// real native bindings\n",
        )
        .expect("write existing native bindings");
        std::fs::write(
            lib_dir.join("demo.dart"),
            conditional_export_shim("demo", "demo"),
        )
        .expect("write shim as lib file");

        move_native_bindings(&lib_dir, &native_dir, "demo").expect("move is a no-op");

        let native = std::fs::read_to_string(native_dir.join("demo.dart")).expect("native file");
        assert_eq!(native, "library demo;\n// real native bindings\n");

        std::fs::remove_dir_all(&package_dir).expect("cleanup temp dir");
    }

    /// If a prior `unify_native_and_web` run moved the file here and then
    /// failed later (e.g. the wasm/web half failed to build), retrying must
    /// not error just because `lib/<package>.dart` no longer exists --
    /// native_dir already has it from the earlier attempt.
    #[test]
    fn move_native_bindings_is_a_no_op_when_already_moved_by_a_prior_failed_attempt() {
        let package_dir = unique_temp_dir("boltffi-move-native-bindings-retry-test");
        let lib_dir = package_dir.join("lib");
        let native_dir = lib_dir.join("src/native");
        std::fs::create_dir_all(&native_dir).expect("create native dir");
        std::fs::write(
            native_dir.join("demo.dart"),
            "library demo;\n// real native bindings\n",
        )
        .expect("write existing native bindings");
        // lib_dir exists but lib/demo.dart does not -- the state left behind
        // by a prior run that moved the file and then failed before writing
        // the shim.
        std::fs::create_dir_all(&lib_dir).expect("create lib dir");

        move_native_bindings(&lib_dir, &native_dir, "demo").expect("move is a no-op");

        let native = std::fs::read_to_string(native_dir.join("demo.dart")).expect("native file");
        assert_eq!(native, "library demo;\n// real native bindings\n");

        std::fs::remove_dir_all(&package_dir).expect("cleanup temp dir");
    }

    #[test]
    fn write_web_setup_doc_lists_the_files_to_copy_and_the_js_namespace() {
        let package_dir = unique_temp_dir("boltffi-write-web-setup-doc-test");
        std::fs::create_dir_all(&package_dir).expect("create package dir");

        write_web_setup_doc(&package_dir, "demo", "demo").expect("doc writes");

        let doc =
            std::fs::read_to_string(package_dir.join("WEB_SETUP.md")).expect("doc is readable");
        assert!(doc.contains("demo_web_loader.mjs"));
        assert!(doc.contains("import 'package:demo/demo.dart';"));
        assert!(doc.contains("__boltffi_demo"));

        std::fs::remove_dir_all(&package_dir).expect("cleanup temp dir");
    }

    #[test]
    fn write_web_setup_doc_references_a_custom_web_module_name() {
        let package_dir = unique_temp_dir("boltffi-write-web-setup-doc-custom-module-test");
        std::fs::create_dir_all(&package_dir).expect("create package dir");

        write_web_setup_doc(&package_dir, "demo", "demo_web").expect("doc writes");

        let doc =
            std::fs::read_to_string(package_dir.join("WEB_SETUP.md")).expect("doc is readable");
        assert!(doc.contains("demo_web_web_loader.mjs"));
        assert!(doc.contains("import 'package:demo/demo.dart';"));
        assert!(doc.contains("`__boltffi_demo_web`"));

        std::fs::remove_dir_all(&package_dir).expect("cleanup temp dir");
    }
}
