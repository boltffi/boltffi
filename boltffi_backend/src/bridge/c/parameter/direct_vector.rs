use boltffi_binding::DirectVectorElementType;

use crate::core::{Error, Result};

use super::super::{C_BRIDGE_CONTRACT, C_BRIDGE_LAYER, Identifier, Type};
use super::{Parameter, ParameterIndex};

/// C ABI parameters that carry one borrowed direct-vector argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DirectVectorParameter {
    name: Identifier,
    pointer: ParameterIndex,
    length: ParameterIndex,
    element: DirectVectorElementAbi,
}

/// C ABI element carried by a direct-vector parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DirectVectorElementAbi {
    /// A typed primitive element pointer plus element count.
    Typed(Type),
    /// Packed direct-record bytes plus byte count.
    PackedBytes,
}

impl DirectVectorParameter {
    /// Returns the source parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the vector pointer parameter position.
    pub const fn pointer(&self) -> ParameterIndex {
        self.pointer
    }

    /// Returns the vector length parameter position.
    pub const fn length(&self) -> ParameterIndex {
        self.length
    }

    /// Returns the vector element ABI.
    pub fn element(&self) -> &DirectVectorElementAbi {
        &self.element
    }

    pub(in crate::bridge::c::parameter) fn from_params(
        params: &[Parameter],
        pointer: usize,
        name: &Identifier,
        element: &DirectVectorElementAbi,
    ) -> Result<Self> {
        let length = pointer + 1;
        let length_role = params.get(length).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "direct-vector parameter group is missing length parameter",
            },
        )?;

        if !length_role.is_direct_vector_length(name) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "direct-vector parameter group has mismatched length parameter",
            });
        }

        Ok(Self {
            name: name.clone(),
            pointer: ParameterIndex::new(pointer),
            length: ParameterIndex::new(length),
            element: element.clone(),
        })
    }
}

impl DirectVectorElementAbi {
    /// Creates the C ABI element for a lowered direct vector.
    pub fn from_binding(element: &DirectVectorElementType) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => {
                Type::primitive(primitive.primitive()).map(Self::Typed)
            }
            DirectVectorElementType::Record(_) => Ok(Self::PackedBytes),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "direct vector element",
            }),
        }
    }

    pub(in crate::bridge::c::parameter) fn pointer_type(&self) -> Type {
        Type::ConstPointer(Box::new(self.element_type()))
    }

    pub(in crate::bridge::c::parameter) fn mutable_pointer_type(&self) -> Type {
        Type::MutPointer(Box::new(self.element_type()))
    }

    fn element_type(&self) -> Type {
        match self {
            Self::Typed(element) => element.clone(),
            Self::PackedBytes => Type::Uint8,
        }
    }

    pub(in crate::bridge::c::parameter) fn length_name(&self, name: &str) -> String {
        match self {
            Self::Typed(_) => format!("{name}_len"),
            Self::PackedBytes => format!("{name}_byte_len"),
        }
    }
}
