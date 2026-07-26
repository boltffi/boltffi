use std::path::PathBuf;

use boltffi_binding::Native;

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, FileLayout, FilePath, GeneratedOutput, Result, bridge, contract::sealed},
};

use super::{PythonExtensionName, contract::PythonCExtBridgeContract, template};

/// CPython C extension bridge backend.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct PythonCExtBridge {
    module: PythonExtensionName,
    path: FilePath,
}

impl PythonCExtBridge {
    /// Creates a CPython C extension bridge.
    pub fn new(module: impl Into<String>, path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            module: PythonExtensionName::parse(module)?,
            path: FilePath::new(path)?,
        })
    }

    /// Creates a CPython C extension bridge using `_native.c`.
    pub fn native_module() -> Result<Self> {
        Self::new("_native", "_native.c")
    }

    /// Returns the CPython extension module name.
    pub fn module(&self) -> &PythonExtensionName {
        &self.module
    }

    /// Returns the generated C source path.
    pub fn path(&self) -> &FilePath {
        &self.path
    }
}

impl bridge::BridgeBackend for PythonCExtBridge {
    type Surface = Native;
    type Input = CBridgeContract;
    type Contract = PythonCExtBridgeContract;

    fn build_contract(&self, input: &Self::Input) -> Result<Self::Contract> {
        PythonCExtBridgeContract::from_c_bridge(self.module.clone(), self.path.clone(), input)
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        let extension = template::Loader::render(contract)?;
        FileLayout::single(self.path.clone()).assemble([Emitted::primary(extension)])
    }
}

impl sealed::BridgeBackend for PythonCExtBridge {}
