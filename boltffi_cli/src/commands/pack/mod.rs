mod all;
mod request;

use crate::cli::Result;
use crate::config::Config;
use crate::reporter::Reporter;

pub use self::request::{
    PackAllOptions, PackAndroidOptions, PackAppleOptions, PackCSharpOptions, PackCommand,
    PackDartOptions, PackExecutionOptions, PackJavaOptions, PackPythonOptions, PackWasmOptions,
};
pub(crate) use crate::pack::android::pack_android;
pub(crate) use crate::pack::apple::pack_apple;
pub(crate) use crate::pack::csharp::pack_csharp;
pub(crate) use crate::pack::dart::pack_dart;
pub(crate) use crate::pack::java::{
    check_java_packaging_prereqs, ensure_java_no_build_supported, pack_java, prepare_java_packaging,
};
pub(crate) use crate::pack::python::pack_python;
pub(crate) use crate::pack::wasm::pack_wasm;

pub fn run_pack(config: &Config, command: PackCommand, reporter: &Reporter) -> Result<()> {
    match command {
        PackCommand::All(options) => all::pack_all(config, options, reporter),
        PackCommand::Apple(options) => pack_apple(config, options, reporter),
        PackCommand::Android(options) => pack_android(config, options, reporter),
        PackCommand::Wasm(options) => pack_wasm(config, options, reporter),
        PackCommand::Java(options) => pack_java(config, options, None, reporter),
        PackCommand::Python(options) => pack_python(config, options, reporter),
        PackCommand::Dart(options) => pack_dart(config, options, reporter),
        PackCommand::CSharp(options) => pack_csharp(config, options, reporter),
    }
}
