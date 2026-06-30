use crate::config::SpmLayout;

pub enum PackCommand {
    All(PackAllOptions),
    Apple(PackAppleOptions),
    Android(PackAndroidOptions),
    Kmp(PackKmpOptions),
    Wasm(PackWasmOptions),
    Java(PackJavaOptions),
    Python(PackPythonOptions),
    Dart(PackDartOptions),
    CSharp(PackCSharpOptions),
    Ruby(PackRubyOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackExecutionOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub cargo_args: Vec<String>,
}

pub struct PackAllOptions {
    pub execution: PackExecutionOptions,
    pub experimental: bool,
    pub python_interpreters: Vec<String>,
}

pub struct PackAppleOptions {
    pub execution: PackExecutionOptions,
    pub version: Option<String>,
    pub spm_only: bool,
    pub xcframework_only: bool,
    pub layout: Option<SpmLayout>,
}

pub struct PackAndroidOptions {
    pub execution: PackExecutionOptions,
}

pub struct PackKmpOptions {
    pub execution: PackExecutionOptions,
    pub experimental: bool,
}

pub struct PackWasmOptions {
    pub execution: PackExecutionOptions,
}

pub struct PackJavaOptions {
    pub execution: PackExecutionOptions,
    pub experimental: bool,
}

pub struct PackPythonOptions {
    pub execution: PackExecutionOptions,
    pub python_interpreters: Vec<String>,
}

pub struct PackDartOptions {
    pub execution: PackExecutionOptions,
    pub experimental: bool,
}

pub struct PackCSharpOptions {
    pub execution: PackExecutionOptions,
}

pub struct PackRubyOptions {
    pub execution: PackExecutionOptions,
    pub targets: Vec<String>,
}
