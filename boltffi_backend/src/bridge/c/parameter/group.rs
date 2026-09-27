use crate::core::{Error, Result};

use super::super::C_BRIDGE_CONTRACT;
use super::{
    ByteSliceParameter, CallbackCompletionParameter, ClosureParameter, ClosureReturnParameter,
    ContinuationParameter, DirectVectorParameter, DirectWritebackParameter,
    EncodedWritebackParameter, Parameter, ParameterIndex, ParameterRole,
};

/// Source-level parameter group represented by one or more C ABI parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParameterGroup {
    /// One source parameter maps to one C ABI parameter.
    Value(ParameterIndex),
    /// One source parameter maps to a borrowed byte pointer and byte length.
    ByteSlice(ByteSliceParameter),
    /// One source parameter maps to a borrowed direct-vector pointer and length.
    DirectVector(DirectVectorParameter),
    /// One mutable encoded source parameter maps to bytes and an output buffer.
    EncodedWriteback(EncodedWritebackParameter),
    /// One fallible success value maps to caller-owned output storage.
    SuccessOut(ParameterIndex),
    /// One status output value maps to bridge-owned status storage.
    CompletionStatusOut(ParameterIndex),
    /// One mutable direct record maps to an input value and output pointer.
    DirectWriteback(DirectWritebackParameter),
    /// One async callback completion maps to callback and context parameters.
    CallbackCompletion(CallbackCompletionParameter),
    /// One poll continuation maps to callback data and a function pointer.
    Continuation(ContinuationParameter),
    /// One closure parameter maps to call, context, and release C ABI parameters.
    Closure(ClosureParameter),
    /// One closure return maps to one caller-owned out-pointer.
    ClosureReturn(ClosureReturnParameter),
}

impl ParameterGroup {
    /// Builds source-level parameter groups from flat C ABI parameters.
    pub fn from_params(params: &[Parameter]) -> Result<Vec<Self>> {
        let mut index = 0;
        std::iter::from_fn(|| {
            (index < params.len()).then(|| {
                let group = Self::from_param(params, index);
                index += group.as_ref().map_or(1, Self::width);
                group
            })
        })
        .collect()
    }

    fn from_param(params: &[Parameter], index: usize) -> Result<Self> {
        if let Some(writeback) = DirectWritebackParameter::from_params(params, index)? {
            return Ok(Self::DirectWriteback(writeback));
        }

        match &params[index].role {
            ParameterRole::Value | ParameterRole::OwnedClass(_) => {
                Ok(Self::Value(ParameterIndex::new(index)))
            }
            ParameterRole::BytePointer(name) => {
                EncodedWritebackParameter::from_params(params, index, name)?
                    .map(Self::EncodedWriteback)
                    .map_or_else(
                        || {
                            ByteSliceParameter::from_params(params, index, name)
                                .map(Self::ByteSlice)
                        },
                        Ok,
                    )
            }
            ParameterRole::ByteLength(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "byte slice parameter group does not start with pointer parameter",
            }),
            ParameterRole::DirectVectorPointer { name, element } => {
                DirectVectorParameter::from_params(params, index, name, element)
                    .map(Self::DirectVector)
            }
            ParameterRole::DirectVectorLength(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "direct-vector parameter group does not start with pointer parameter",
            }),
            ParameterRole::EncodedWriteback(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "encoded writeback parameter group does not start with pointer parameter",
            }),
            ParameterRole::SuccessOut => Ok(Self::SuccessOut(ParameterIndex::new(index))),
            ParameterRole::CompletionStatusOut => {
                Ok(Self::CompletionStatusOut(ParameterIndex::new(index)))
            }
            ParameterRole::CallbackCompletionCallback(name) => {
                CallbackCompletionParameter::from_params(params, index, name)
                    .map(Self::CallbackCompletion)
            }
            ParameterRole::CallbackCompletionContext(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter group does not start with callback parameter",
            }),
            ParameterRole::ContinuationData(name) => {
                ContinuationParameter::from_params(params, index, name).map(Self::Continuation)
            }
            ParameterRole::ContinuationCallback(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "continuation parameter group does not start with data parameter",
            }),
            ParameterRole::ClosureCall {
                name,
                signature,
                parameters,
                return_channel,
            } => ClosureParameter::from_params(
                params,
                index,
                name,
                signature,
                parameters,
                *return_channel,
            )
            .map(Self::Closure),
            ParameterRole::ClosureContext(_) | ParameterRole::ClosureRelease(_) => {
                Err(Error::BrokenBridgeContract {
                    bridge: C_BRIDGE_CONTRACT,
                    invariant: "closure parameter group does not start with call parameter",
                })
            }
            ParameterRole::ClosureReturn {
                name,
                signature,
                call_type,
                parameters,
                return_channel,
            } => ClosureReturnParameter::from_params(
                params,
                index,
                name,
                signature,
                call_type,
                parameters,
                *return_channel,
            )
            .map(Self::ClosureReturn),
        }
    }

    fn width(&self) -> usize {
        match self {
            Self::Value(_) => 1,
            Self::ByteSlice(_) => 2,
            Self::DirectVector(_) => 2,
            Self::EncodedWriteback(_) => 3,
            Self::SuccessOut(_) => 1,
            Self::CompletionStatusOut(_) => 1,
            Self::DirectWriteback(_) => 2,
            Self::CallbackCompletion(_) => 2,
            Self::Continuation(_) => 2,
            Self::Closure(_) => 3,
            Self::ClosureReturn(_) => 1,
        }
    }
}
