use boltffi_binding::ClosureSignature;

use crate::core::{Error, Result};

use super::super::{C_BRIDGE_CONTRACT, Identifier, ReturnChannel, Type};
use super::{Parameter, ParameterGroup, ParameterIndex};

/// C ABI out-pointer that receives one returned closure registration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureReturnParameter {
    name: Identifier,
    signature: ClosureSignature,
    output: ParameterIndex,
    call_type: Type,
    parameters: Vec<Parameter>,
    return_channel: ReturnChannel,
    parameter_groups: Vec<ParameterGroup>,
}

impl ClosureReturnParameter {
    pub(crate) fn return_channel(&self) -> ReturnChannel {
        self.return_channel
    }

    /// Returns the output parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the returned closure invocation signature.
    pub fn signature(&self) -> &ClosureSignature {
        &self.signature
    }

    /// Returns the output parameter position.
    pub const fn output(&self) -> ParameterIndex {
        self.output
    }

    /// Returns the returned closure invoke function-pointer type.
    pub fn call_type(&self) -> &Type {
        &self.call_type
    }

    /// Returns the C ABI parameter at the given returned-closure call position.
    pub fn parameter(&self, index: ParameterIndex) -> &Parameter {
        &self.parameters[index.position()]
    }

    /// Returns source-level returned-closure call parameter groups in declaration order.
    pub fn parameter_groups(&self) -> &[ParameterGroup] {
        &self.parameter_groups
    }

    /// Builds a closure return out-pointer from a flat C ABI parameter list.
    pub fn from_params(
        params: &[Parameter],
        output: usize,
        name: &Identifier,
        signature: &ClosureSignature,
        call_type: &Type,
        parameters: &[Parameter],
        return_channel: ReturnChannel,
    ) -> Result<Self> {
        let Some(parameter) = params.get(output) else {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure return parameter group is missing output parameter",
            });
        };
        if !matches!(parameter.ty(), Type::MutPointer(inner) if matches!(inner.as_ref(), Type::Void))
        {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure return parameter is not a void out-pointer",
            });
        }
        Ok(Self {
            name: name.clone(),
            signature: signature.clone(),
            output: ParameterIndex::new(output),
            call_type: call_type.clone(),
            parameters: parameters.to_vec(),
            return_channel,
            parameter_groups: ParameterGroup::from_params(parameters)?,
        })
    }
}
