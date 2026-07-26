use boltffi_binding::{ConstantDecl, ConstantValueDecl, Native};

use crate::{
    bridge::python_cext::{ExtensionMethod, PythonCExtBridgeContract},
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::{
            codec,
            render::{direct_vector, function, primitive, result},
        },
        name_style::Name,
    },
};

pub struct Constant {
    function: Option<function::Function>,
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let function = match declaration.value() {
            ConstantValueDecl::Inline { .. } => None,
            ConstantValueDecl::Accessor { symbol, callable } => {
                Some(function::Function::from_export(
                    Name::new(declaration.name()).function()?,
                    symbol,
                    callable,
                    Vec::new(),
                    bridge,
                    context,
                )?)
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown constant value",
                });
            }
        };
        Ok(Self { function })
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        self.function
            .as_ref()
            .into_iter()
            .flat_map(function::Function::methods)
    }

    pub fn render(self) -> Result<Emitted> {
        match self.function {
            Some(function) => function.render(),
            None => Ok(Emitted::primary("")),
        }
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        self.function
            .as_ref()
            .map(function::Function::primitives)
            .unwrap_or_default()
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.function
            .as_ref()
            .into_iter()
            .flat_map(function::Function::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.function
            .as_ref()
            .into_iter()
            .flat_map(function::Function::direct_vector_elements)
    }

    pub fn native_sequences(&self) -> impl Iterator<Item = codec::NativeSequence> + '_ {
        self.function
            .as_ref()
            .into_iter()
            .flat_map(function::Function::native_sequences)
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.function
            .as_ref()
            .and_then(function::Function::owned_buffer)
    }

    pub fn has_string_argument(&self) -> bool {
        self.function
            .as_ref()
            .is_some_and(function::Function::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.function
            .as_ref()
            .is_some_and(function::Function::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.function
            .as_ref()
            .is_some_and(function::Function::has_raw_wire_argument)
    }
}
