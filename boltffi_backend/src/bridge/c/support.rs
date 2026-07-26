use crate::core::{Error, Result};

use super::{C_BRIDGE_LAYER, Function, Parameter, Type};

/// C ABI support functions supplied by the BoltFFI runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SupportFunctions {
    functions: Vec<Function>,
}

impl SupportFunctions {
    /// Creates the C ABI support function set.
    pub fn new() -> Result<Self> {
        Ok(Self {
            functions: vec![
                Function::new(
                    "boltffi_free_string",
                    vec![Parameter::new("string", Type::String)?],
                    Type::Void,
                )?,
                Function::new(
                    "boltffi_free_buf",
                    vec![Parameter::new("buf", Type::Buffer)?],
                    Type::Void,
                )?,
                Function::new(
                    "boltffi_buf_from_bytes",
                    vec![
                        Parameter::new("ptr", Type::ConstPointer(Box::new(Type::Uint8)))?,
                        Parameter::new("len", Type::PointerWidth)?,
                    ],
                    Type::Buffer,
                )?,
                Function::new(
                    "boltffi_buf_with_len",
                    vec![Parameter::new("len", Type::PointerWidth)?],
                    Type::Buffer,
                )?,
                Function::new(
                    "boltffi_last_error_message",
                    vec![Parameter::new(
                        "out",
                        Type::MutPointer(Box::new(Type::String)),
                    )?],
                    Type::Status,
                )?,
                Function::new("boltffi_clear_last_error", Vec::new(), Type::Void)?,
            ],
        })
    }

    /// Returns C ABI support functions.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// Returns the C ABI support function that releases a BoltFFI buffer.
    pub fn buffer_free(&self) -> Result<&Function> {
        self.function("boltffi_free_buf", "missing C free buffer support symbol")
    }

    /// Returns the C ABI support function that copies bytes into a BoltFFI buffer.
    pub fn buffer_from_bytes(&self) -> Result<&Function> {
        self.function(
            "boltffi_buf_from_bytes",
            "missing C buffer copy support symbol",
        )
    }

    pub(crate) fn buffer_with_len(&self) -> Result<&Function> {
        self.function(
            "boltffi_buf_with_len",
            "missing C buffer allocation support symbol",
        )
    }

    fn function(&self, name: &str, shape: &'static str) -> Result<&Function> {
        self.functions
            .iter()
            .find(|function| function.name() == name)
            .ok_or(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape,
            })
    }
}
