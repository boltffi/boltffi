//! Packages the `dart_web` (js_interop over wasm) target: runs `pack wasm`
//! unmodified, then vendors the result so it needs zero npm/Node.js on the
//! consuming side (the primary use case, a Flutter web app, has no npm step
//! at all). `@boltffi/runtime` ships as the pre-built JS in
//! `vendor/runtime-js` (see `scripts/sync-dart-web-runtime.sh`/`.ps1`).

use std::fs;
use std::path::Path;

use boltffi_backend::target::dart_web::DartWebHost;

use crate::{
    cli::{CliError, Result},
    commands::{
        generate::{GenerateOptions, GenerateTarget, run_generate_with_output},
        pack::{PackDartWebOptions, PackExecutionOptions, PackWasmOptions, pack_wasm},
    },
    config::{Config, WasmNpmTarget},
    reporter::Reporter,
};

const RUNTIME_SOURCES: &[(&str, &str)] = &[
    ("callback.js", include_str!("../../../vendor/runtime-js/callback.js")),
    ("handle.js", include_str!("../../../vendor/runtime-js/handle.js")),
    ("index.js", include_str!("../../../vendor/runtime-js/index.js")),
    ("module.js", include_str!("../../../vendor/runtime-js/module.js")),
    ("stream.js", include_str!("../../../vendor/runtime-js/stream.js")),
    ("wire.js", include_str!("../../../vendor/runtime-js/wire.js")),
];

pub(crate) fn pack_dart_web(
    config: &Config,
    options: PackDartWebOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_dart_web_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.dart_web.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🕸️", "Packing Dart Web");
    pack_wrapped_wasm_module(config, &options.execution, reporter)?;
    generate_and_vendor_web(
        config,
        &options.execution,
        options.experimental,
        &config.dart_web_output(),
        reporter,
    )?;
    reporter.finish();
    Ok(())
}

pub(crate) fn pack_wrapped_wasm_module(
    config: &Config,
    execution: &PackExecutionOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.wasm_npm_targets().contains(&WasmNpmTarget::Web) {
        return Err(CliError::CommandFailed {
            command: "targets.dart_web requires \"web\" in targets.wasm.npm.targets \
                      (the generated Dart file calls into that browser entrypoint)"
                .to_string(),
            status: None,
        });
    }

    let step = reporter.step("Packing wrapped WASM/TypeScript module");
    pack_wasm(
        config,
        PackWasmOptions {
            execution: execution.clone(),
            require_npm_metadata: false,
        },
        reporter,
    )?;
    step.finish_success();
    Ok(())
}

// Assumes `pack_wrapped_wasm_module` already ran. Returns the JS
// namespace the loader publishes and the generated Dart file's `@JS()`
// externs bind to.
pub(crate) fn generate_and_vendor_web(
    config: &Config,
    execution: &PackExecutionOptions,
    experimental: bool,
    output_directory: &Path,
    reporter: &Reporter,
) -> Result<String> {
    let module_name = config.dart_web_module_name();
    let namespace = DartWebHost::new(&module_name)
        .map_err(|error| CliError::CommandFailed {
            command: format!("dart_web module name '{module_name}' is invalid: {error}"),
            status: None,
        })?
        .js_namespace();

    if execution.regenerate {
        let step = reporter.step("Generating Dart bindings");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::DartWeb,
                output: Some(output_directory.to_path_buf()),
                experimental,
                cargo_args: execution.cargo_args.clone(),
                deny_skipped: execution.deny_skipped,
            },
        )?;
        step.finish_success();
    }

    vendor_web_assets(config, output_directory, reporter)?;

    {
        let step = reporter.step("Generating the JS loader");
        let loader_path = output_directory.join(format!("{module_name}_web_loader.mjs"));
        let loader_source = render_loader_script(&namespace);
        fs::write(&loader_path, loader_source).map_err(|source| CliError::WriteFailed {
            path: loader_path.clone(),
            source,
        })?;
        step.finish_success();
    }

    Ok(namespace)
}

fn vendor_web_assets(config: &Config, output_directory: &Path, reporter: &Reporter) -> Result<()> {
    let npm_output_directory = config.wasm_npm_output();
    let wasm_module_name = config.wasm_typescript_module_name();
    let web_directory = output_directory.join("web");
    fs::create_dir_all(&web_directory).map_err(|source| CliError::CreateDirectoryFailed {
        path: web_directory.clone(),
        source,
    })?;

    let step = reporter.step("Vendoring the wrapped WASM/JS module");

    for file_name in [format!("{wasm_module_name}_bg.wasm"), "web.js".to_owned()] {
        let from = npm_output_directory.join(&file_name);
        let to = web_directory.join(&file_name);
        fs::copy(&from, &to).map_err(|source| CliError::CopyFailed { from, to, source })?;
    }

    let bindings_file_name = format!("{wasm_module_name}.js");
    let bindings_source = fs::read_to_string(npm_output_directory.join(&bindings_file_name))
        .map_err(|source| CliError::ReadFailed {
            path: npm_output_directory.join(&bindings_file_name),
            source,
        })?;
    let runtime_import = format!("\"{}\"", config.wasm_runtime_package());
    let rewritten_bindings =
        bindings_source.replace(&runtime_import, "\"./boltffi_runtime/index.js\"");
    let bindings_dest = web_directory.join(&bindings_file_name);
    fs::write(&bindings_dest, rewritten_bindings).map_err(|source| CliError::WriteFailed {
        path: bindings_dest,
        source,
    })?;

    let runtime_directory = web_directory.join("boltffi_runtime");
    fs::create_dir_all(&runtime_directory).map_err(|source| CliError::CreateDirectoryFailed {
        path: runtime_directory.clone(),
        source,
    })?;
    for (name, source) in RUNTIME_SOURCES {
        let path = runtime_directory.join(name);
        fs::write(&path, source).map_err(|source| CliError::WriteFailed { path, source })?;
    }

    step.finish_success();
    Ok(())
}

// Must stay in sync with `DartWebHost::js_namespace` /
// `render::Module::ready_global` in `boltffi_backend::target::dart_web`.
fn render_loader_script(namespace: &str) -> String {
    format!(
        "// Generated by `boltffi pack dart-web`. Do not edit by hand.\n\
         //\n\
         // Load this script (as an ES module) before the compiled Dart\n\
         // output — e.g. from index.html:\n\
         //   <script type=\"module\" src=\"{module}_web_loader.mjs\"></script>\n\
         //   <script defer src=\"main.dart.js\"></script>\n\
         import * as __boltffiModule from \"./web/web.js\";\n\n\
         globalThis[\"{namespace}\"] = __boltffiModule;\n\
         globalThis[\"{namespace}_ready\"] = __boltffiModule.initialized;\n",
        module = namespace.trim_start_matches("__boltffi_"),
    )
}

#[cfg(test)]
mod tests {
    use super::render_loader_script;

    #[test]
    fn loader_publishes_the_namespace_and_ready_globals() {
        let script = render_loader_script("__boltffi_demo");

        assert!(script.contains("import * as __boltffiModule from \"./web/web.js\";"));
        assert!(script.contains("globalThis[\"__boltffi_demo\"] = __boltffiModule;"));
        assert!(
            script.contains("globalThis[\"__boltffi_demo_ready\"] = __boltffiModule.initialized;")
        );
    }
}
