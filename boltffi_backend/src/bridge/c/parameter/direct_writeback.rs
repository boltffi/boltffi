use crate::core::{Error, Result};

use super::super::{C_BRIDGE_CONTRACT, Identifier, Type};
use super::{Parameter, ParameterIndex, ParameterRole};

/// C ABI parameters that carry one direct record plus a mutation writeback pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DirectWritebackParameter {
    name: Identifier,
    input: ParameterIndex,
    output: ParameterIndex,
}

impl DirectWritebackParameter {
    /// Returns the source parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the direct record input parameter position.
    pub const fn input(&self) -> ParameterIndex {
        self.input
    }

    /// Returns the mutation writeback output parameter position.
    pub const fn output(&self) -> ParameterIndex {
        self.output
    }

    pub(in crate::bridge::c::parameter) fn from_params(
        params: &[Parameter],
        input: usize,
    ) -> Result<Option<Self>> {
        let parameter = &params[input];
        if !matches!(parameter.role, ParameterRole::Value)
            || !matches!(
                parameter.ty(),
                Type::DirectRecord(_) | Type::CStyleEnum { .. }
            )
        {
            return Ok(None);
        }

        let Some(output) = params.get(input + 1) else {
            return Ok(None);
        };
        let expected_output = format!("{}_out", parameter.name());
        if output.name() != expected_output {
            return Ok(None);
        }
        if !matches!(output.role, ParameterRole::Value) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "direct writeback output parameter has unexpected role",
            });
        }
        let Type::MutPointer(output_type) = output.ty() else {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "direct writeback output parameter is not mutable pointer",
            });
        };
        if output_type.as_ref() != parameter.ty() {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "direct writeback output parameter has mismatched type",
            });
        }

        Ok(Some(Self {
            name: Identifier::escape(parameter.name())?,
            input: ParameterIndex::new(input),
            output: ParameterIndex::new(input + 1),
        }))
    }
}
