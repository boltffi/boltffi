use crate::config::SpmLayout;

pub enum PackCommand {
    All(PackAllOptions),
    Apple(PackAppleOptions),
    Android(PackAndroidOptions),
    Wasm(PackWasmOptions),
    Java(PackJavaOptions),
    Python(PackPythonOptions),
}

pub struct PackAllOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub experimental: bool,
    pub python_interpreters: Vec<String>,
    pub cargo_args: Vec<String>,
    /// See [`crate::build::CargoBuildCommand::parse`] for accepted values.
    pub cargo_build_cmd: Option<String>,
}

pub struct PackAppleOptions {
    pub release: bool,
    pub version: Option<String>,
    pub regenerate: bool,
    pub no_build: bool,
    pub spm_only: bool,
    pub xcframework_only: bool,
    pub layout: Option<SpmLayout>,
    pub cargo_args: Vec<String>,
    /// See [`crate::build::CargoBuildCommand::parse`] for accepted values.
    pub cargo_build_cmd: Option<String>,
}

pub struct PackAndroidOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub cargo_args: Vec<String>,
    /// See [`crate::build::CargoBuildCommand::parse`] for accepted values.
    pub cargo_build_cmd: Option<String>,
}

pub struct PackWasmOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub cargo_args: Vec<String>,
    /// See [`crate::build::CargoBuildCommand::parse`] for accepted values.
    pub cargo_build_cmd: Option<String>,
}

pub struct PackJavaOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub experimental: bool,
    pub cargo_args: Vec<String>,
    /// See [`crate::build::CargoBuildCommand::parse`] for accepted values.
    pub cargo_build_cmd: Option<String>,
}

pub struct PackPythonOptions {
    pub release: bool,
    pub regenerate: bool,
    pub no_build: bool,
    pub experimental: bool,
    pub python_interpreters: Vec<String>,
    pub cargo_args: Vec<String>,
}
