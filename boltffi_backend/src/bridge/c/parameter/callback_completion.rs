use crate::{
    bridge::c::Type,
    core::{Error, Result},
};

use super::super::{C_BRIDGE_CONTRACT, Identifier};
use super::{Parameter, ParameterIndex};

/// C ABI parameters that carry one async callback completion.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionParameter {
    name: Identifier,
    callback: ParameterIndex,
    context: ParameterIndex,
    payload: Option<Type>,
}

impl CallbackCompletionParameter {
    /// Returns the source completion name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the callback function pointer parameter position.
    pub const fn callback(&self) -> ParameterIndex {
        self.callback
    }

    /// Returns the callback context parameter position.
    pub const fn context(&self) -> ParameterIndex {
        self.context
    }

    /// Returns the payload type passed after status.
    pub fn payload(&self) -> Option<&Type> {
        self.payload.as_ref()
    }

    pub(in crate::bridge::c::parameter) fn from_params(
        params: &[Parameter],
        callback: usize,
        name: &Identifier,
    ) -> Result<Self> {
        let context = callback + 1;
        let context_role = params.get(context).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter group is missing context parameter",
            },
        )?;

        if !context_role.is_callback_completion_context(name) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter group has mismatched context parameter",
            });
        }

        Ok(Self {
            name: name.clone(),
            callback: ParameterIndex::new(callback),
            context: ParameterIndex::new(context),
            payload: Self::payload_type(params, callback)?,
        })
    }

    fn payload_type(params: &[Parameter], callback: usize) -> Result<Option<Type>> {
        let callback = params.get(callback).ok_or(Error::BrokenBridgeContract {
            bridge: C_BRIDGE_CONTRACT,
            invariant: "callback completion parameter group is missing callback parameter",
        })?;
        let Type::FunctionPointer {
            returns,
            params: arguments,
        } = callback.ty()
        else {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter is not a function pointer",
            });
        };
        if !matches!(returns.as_ref(), Type::Void) || !Self::starts_with_context_status(arguments) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion function pointer has unsupported signature",
            });
        }
        match arguments.as_slice() {
            [_, _] => Ok(None),
            [_, _, payload] => Ok(Some(payload.clone())),
            _ => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion function pointer has too many payload parameters",
            }),
        }
    }

    fn starts_with_context_status(arguments: &[Type]) -> bool {
        matches!(
            arguments,
            [Type::MutPointer(context), Type::Status, ..] if matches!(context.as_ref(), Type::Void)
        )
    }
}
