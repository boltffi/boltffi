//! Backend entry point for one generated JNI source file.
//!
//! A JVM target places `JniBridge` above a `CBridge` in the backend stack. The
//! target supplies only the JVM-facing choices: package name, owner class, and
//! generated file path. The Rust ABI still comes from the lower C bridge
//! contract, including symbol names, record layouts, callback vtables, stream
//! protocols, and buffer release functions.
//!
//! Rendering is a two-step operation. First, the bridge builds a
//! `JniBridgeContract` from the C bridge contract. Then the template layer turns
//! that contract into a C file containing `Java_*` native methods, callback
//! dispatchers, closure trampolines, stream helpers, async completion glue, and
//! the support code needed to call the C ABI safely from JNI.

use std::path::PathBuf;

use boltffi_binding::Native;

use crate::{
    bridge::{
        c::CBridgeContract,
        jni::{JniBridgeContract, JvmClassPath, template},
    },
    core::{Emitted, FileLayout, FilePath, GeneratedOutput, Result, bridge, contract::sealed},
};

/// A JNI bridge backend layered above the C ABI bridge.
///
/// The bridge produces one C source file for a JVM owner class. It expects the
/// lower bridge to provide the C ABI contract and only adds the JNI adaptation:
/// `Java_*` symbols, JVM descriptors, callback dispatch, closure trampolines,
/// stream helpers, and lifecycle registration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct JniBridge {
    class: JvmClassPath,
    path: FilePath,
}

impl JniBridge {
    /// Creates a JNI bridge.
    pub fn new(
        package: impl Into<String>,
        class: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<Self> {
        Ok(Self {
            class: JvmClassPath::new(package, class)?,
            path: FilePath::new(path)?,
        })
    }

    /// Creates a JNI bridge that writes `jni_glue.c` for a `Native` JVM class.
    pub fn native_class(package: impl Into<String>) -> Result<Self> {
        Self::new(package, "Native", "jni_glue.c")
    }

    /// Returns the JVM class that owns native methods.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the generated JNI source path.
    pub fn path(&self) -> &FilePath {
        &self.path
    }
}

impl bridge::BridgeBackend for JniBridge {
    type Surface = Native;
    type Input = CBridgeContract;
    type Contract = JniBridgeContract;

    fn build_contract(&self, input: &Self::Input) -> Result<Self::Contract> {
        JniBridgeContract::from_c_bridge(self.class.clone(), self.path.clone(), input)
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        let source = template::SourceFile::render(contract)?;
        FileLayout::single(self.path.clone()).assemble([Emitted::primary(source)])
    }
}

impl sealed::BridgeBackend for JniBridge {}
