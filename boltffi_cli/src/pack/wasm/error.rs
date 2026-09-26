use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to render Wasm package: {0}")]
    Template(#[from] askama::Error),
    #[error("invalid Wasm module: {0}")]
    Module(#[from] wasmparser::BinaryReaderError),
    #[error("invalid wasm-bindgen metadata: {0}")]
    Metadata(String),
    #[error(
        "Wasm import {module}::{name} requires wasm-bindgen metadata, but it is missing; rebuild the raw Cargo artifact before packing"
    )]
    MissingMetadata { module: String, name: String },
    #[error(
        "wasm-bindgen {version} is required; set targets.wasm.wasm_bindgen_cli or BOLTFFI_WASM_BINDGEN to its executable, or install it with `cargo install wasm-bindgen-cli --version ={version} --locked`"
    )]
    MissingTool { version: semver::Version },
    #[error(
        "wasm-bindgen at {path} reports {actual}; the Wasm artifact requires {expected}; install the matching CLI or set BOLTFFI_WASM_BINDGEN"
    )]
    ToolVersion {
        path: PathBuf,
        expected: semver::Version,
        actual: String,
    },
    #[error("unsupported Wasm packaging contract: {0}")]
    Unsupported(String),
    #[error("Wasm function references missing type {index}")]
    MissingFunctionType { index: u32 },
    #[error("Wasm export {name} references missing function {index}")]
    MissingExportFunction { name: String, index: u32 },
    #[error("Wasm processing removed export {name} or changed its signature")]
    ExportChanged { name: String },
    #[error("Wasm processing added env import {name} or changed its signature")]
    EnvironmentImportChanged { name: String },
    #[error("unresolved Wasm import {module}::{name}: {reason}")]
    Import {
        module: String,
        name: String,
        reason: String,
    },
}
