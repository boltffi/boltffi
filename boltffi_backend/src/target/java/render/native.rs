use std::fmt;

use askama::Template as AskamaTemplate;
use boltffi_binding::NativeSymbol;

use crate::{
    bridge::jni::{
        CallbackCompletionInvoker, CallbackCompletionPayload, CallbackCompletionPayloadValue,
        CallbackHandleLifecycle, CallbackHandleMethod, DirectStreamBatchMethod, JniBridgeContract,
        NativeMethod, NativeParameter, NativeParameterKind, NativeReturn, SuccessOutValue,
        SuccessOutWriter,
    },
    core::{Error, Result},
    target::java::{
        JavaVersion,
        primitive::Primitive,
        render::signature::{Parameter, ReturnType, ValueType},
        syntax::{ArgumentList, Expression, Identifier, TypeIdentifier, TypeName},
    },
    target::jvm::method::{Parameter as JvmParameter, Parameters as JvmParameters, SlotWidth},
};

#[derive(AskamaTemplate)]
#[template(path = "target/java/native_method.java", escape = "none")]
struct MethodTemplate<'method> {
    method: &'method Method,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Method {
    name: Identifier,
    parameters: JvmParameters<Parameter<Carrier>>,
    returns: MethodReturn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Carrier {
    Primitive(Primitive),
    PrimitiveArray(Primitive),
    ByteArray,
    DirectBuffer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MethodReturn {
    Void,
    Value(Carrier),
    CheckedVoid,
}

impl Method {
    pub fn from_symbol(
        symbol: &NativeSymbol,
        bridge: &JniBridgeContract,
        version: JavaVersion,
    ) -> Result<Self> {
        bridge
            .source_method(symbol.id())
            .ok_or(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "source symbol has no JNI native method",
            })
            .and_then(|method| Self::from_contract(method, version))
    }

    pub fn from_contract(method: &NativeMethod, version: JavaVersion) -> Result<Self> {
        let name = Identifier::parse_for(method.c_function().name(), version)?;
        let parameters = method
            .parameters()
            .iter()
            .map(|parameter| Self::parameter_from_contract(parameter, version))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        Self::new(
            name,
            parameters,
            MethodReturn::from_contract(method.returns())?,
        )
    }

    pub fn from_direct_stream_batch(
        method: &DirectStreamBatchMethod,
        version: JavaVersion,
    ) -> Result<Self> {
        Self::new(
            Identifier::parse_for(method.c_function().name(), version)?,
            vec![
                Parameter::new(
                    Identifier::known("subscription"),
                    Carrier::Primitive(Primitive::Long),
                ),
                Parameter::new(
                    Identifier::known("maxCount"),
                    Carrier::Primitive(Primitive::Long),
                ),
            ],
            MethodReturn::Value(Carrier::ByteArray),
        )
    }

    pub fn from_callback_handle_method(
        method: &CallbackHandleMethod,
        version: JavaVersion,
    ) -> Result<Self> {
        let returns = match method.synchronous_return() {
            Some(returns) => MethodReturn::from_contract(returns)?,
            None if method.returns_closure() => {
                MethodReturn::Value(Carrier::Primitive(Primitive::Long))
            }
            None => MethodReturn::Void,
        };
        let completion = method
            .completion()
            .map(|completion| -> Result<_> {
                Ok(Parameter::new(
                    Identifier::escape_for(completion.context().as_str(), version)?,
                    Carrier::Primitive(Primitive::Long),
                ))
            })
            .transpose()?;
        let parameters = std::iter::once(Parameter::new(
            Identifier::known("handle"),
            Carrier::Primitive(Primitive::Long),
        ))
        .chain(
            method
                .parameters()
                .iter()
                .map(|parameter| Self::parameter_from_contract(parameter, version))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten(),
        )
        .chain(completion)
        .collect();
        Self::new(
            Identifier::parse_for(method.method().as_str(), version)?,
            parameters,
            returns,
        )
    }

    pub fn from_callback_handle_lifecycle(
        lifecycle: &CallbackHandleLifecycle,
        version: JavaVersion,
    ) -> Result<Vec<Self>> {
        [
            (
                lifecycle.clone_method().as_str(),
                MethodReturn::Value(Carrier::Primitive(Primitive::Long)),
            ),
            (lifecycle.release_method().as_str(), MethodReturn::Void),
        ]
        .into_iter()
        .map(|(name, returns)| {
            Self::new(
                Identifier::parse_for(name, version)?,
                vec![Parameter::new(
                    Identifier::known("handle"),
                    Carrier::Primitive(Primitive::Long),
                )],
                returns,
            )
        })
        .collect()
    }

    pub fn from_callback_completion(
        invoker: &CallbackCompletionInvoker,
        version: JavaVersion,
    ) -> Result<Vec<Self>> {
        Ok([
            Some(Self::callback_completion_method(
                invoker.success_method().as_str(),
                invoker.payload(),
                version,
            )?),
            Some(Self::callback_completion_method(
                invoker.failure_method().as_str(),
                None,
                version,
            )?),
            invoker
                .error_method()
                .map(|method| {
                    Self::callback_completion_method(method.as_str(), invoker.payload(), version)
                })
                .transpose()?,
        ]
        .into_iter()
        .flatten()
        .collect())
    }

    pub fn from_success_out_writer(
        writer: &SuccessOutWriter,
        version: JavaVersion,
    ) -> Result<Self> {
        Self::new(
            Identifier::parse_for(writer.method().as_str(), version)?,
            vec![
                Parameter::new(
                    Identifier::known("returnOut"),
                    Carrier::Primitive(Primitive::Long),
                ),
                Parameter::new(
                    Identifier::known("value"),
                    Carrier::from_success_out(writer.value()),
                ),
            ],
            MethodReturn::Void,
        )
    }

    pub fn validate_return(&self, returns: &ReturnType) -> Result<()> {
        self.returns.validate_carrier(returns)
    }

    pub fn call(
        &self,
        owner: &TypeIdentifier,
        arguments: impl IntoIterator<Item = Expression>,
    ) -> Result<Expression> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        if arguments.len() != self.parameters.len() {
            return Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "Java native call argument arity matches",
            });
        }
        Ok(Expression::static_call(
            TypeName::named(owner.clone()),
            self.name.clone(),
            arguments.into_iter().collect::<ArgumentList>(),
        ))
    }

    pub fn render(&self) -> Result<String> {
        Ok(MethodTemplate { method: self }.render()?)
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn parameters(&self) -> &[Parameter<Carrier>] {
        self.parameters.as_slice()
    }

    fn returns(&self) -> &MethodReturn {
        &self.returns
    }

    fn parameter_from_contract(
        parameter: &NativeParameter,
        version: JavaVersion,
    ) -> Result<Vec<Parameter<Carrier>>> {
        match parameter.kind() {
            NativeParameterKind::Scalar(scalar) => Ok(vec![Parameter::new(
                Identifier::escape_for(parameter.name().as_str(), version)?,
                Carrier::Primitive(Primitive::from(scalar.ty())),
            )]),
            NativeParameterKind::Bytes(bytes) => Ok(vec![
                Parameter::new(
                    Identifier::escape_for(bytes.name().as_str(), version)?,
                    Carrier::DirectBuffer,
                ),
                Parameter::new(
                    Identifier::escape_for(bytes.length().as_str(), version)?,
                    Carrier::Primitive(Primitive::Int),
                ),
            ]),
            NativeParameterKind::Record(_) => Ok(vec![Parameter::new(
                Identifier::escape_for(parameter.name().as_str(), version)?,
                Carrier::DirectBuffer,
            )]),
            NativeParameterKind::DirectVector(vector) => Ok(vec![Parameter::new(
                Identifier::escape_for(vector.name().as_str(), version)?,
                Carrier::PrimitiveArray(Primitive::from(vector.jni_type())),
            )]),
            NativeParameterKind::Callback(callback) => Ok(vec![Parameter::new(
                Identifier::escape_for(callback.name().as_str(), version)?,
                Carrier::Primitive(Primitive::Long),
            )]),
            NativeParameterKind::Closure(closure) => Ok(vec![Parameter::new(
                Identifier::escape_for(closure.name().as_str(), version)?,
                Carrier::Primitive(Primitive::Long),
            )]),
            NativeParameterKind::Continuation(continuation) => Ok(vec![Parameter::new(
                Identifier::escape_for(continuation.name().as_str(), version)?,
                Carrier::Primitive(Primitive::Long),
            )]),
        }
    }

    fn new(
        name: Identifier,
        parameters: Vec<Parameter<Carrier>>,
        returns: MethodReturn,
    ) -> Result<Self> {
        Parameter::validate_unique(name.as_str(), &parameters)?;
        Ok(Self {
            name,
            parameters: JvmParameters::for_static(parameters)?,
            returns,
        })
    }

    fn callback_completion_method(
        name: &str,
        payload: Option<&CallbackCompletionPayload>,
        version: JavaVersion,
    ) -> Result<Self> {
        Self::new(
            Identifier::parse_for(name, version)?,
            [
                Parameter::new(
                    Identifier::known("callback"),
                    Carrier::Primitive(Primitive::Long),
                ),
                Parameter::new(
                    Identifier::known("context"),
                    Carrier::Primitive(Primitive::Long),
                ),
            ]
            .into_iter()
            .chain(payload.map(|payload| {
                Parameter::new(
                    Identifier::known("result"),
                    Carrier::from_completion(payload.value()),
                )
            }))
            .collect(),
            MethodReturn::Void,
        )
    }
}

impl MethodReturn {
    fn from_contract(returns: &NativeReturn) -> Result<Self> {
        match returns {
            NativeReturn::Void => Ok(Self::Void),
            NativeReturn::Status | NativeReturn::EncodedError => Ok(Self::CheckedVoid),
            NativeReturn::Value(scalar) => Ok(Self::Value(Carrier::Primitive(Primitive::from(
                scalar.jni_type(),
            )))),
            NativeReturn::Callback(_) => Ok(Self::Value(Carrier::Primitive(Primitive::Long))),
            NativeReturn::Bytes | NativeReturn::Record(_) | NativeReturn::StatusWriteback(_) => {
                Ok(Self::Value(Carrier::ByteArray))
            }
            NativeReturn::StatusValue(value) => Self::from_contract(value.value()),
            NativeReturn::EncodedErrorValue(value) => Self::from_contract(value.success().value()),
        }
    }

    fn validate_carrier(&self, returns: &ReturnType) -> Result<()> {
        match self {
            Self::Void if matches!(returns, ReturnType::Void) => Ok(()),
            Self::Value(native) if matches!(returns, ReturnType::Value(value) if native.matches(value)) => {
                Ok(())
            }
            Self::CheckedVoid if matches!(returns, ReturnType::Void) => Ok(()),
            _ => Err(Error::BrokenBridgeContract {
                bridge: "jni",
                invariant: "Java and JNI function return shapes match",
            }),
        }
    }
}

impl fmt::Display for MethodReturn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void | Self::CheckedVoid => formatter.write_str("void"),
            Self::Value(returns) => returns.fmt(formatter),
        }
    }
}

impl Carrier {
    fn from_completion(value: CallbackCompletionPayloadValue) -> Self {
        match value {
            CallbackCompletionPayloadValue::Scalar(ty) => Self::Primitive(Primitive::from(ty)),
            CallbackCompletionPayloadValue::Bytes | CallbackCompletionPayloadValue::Record => {
                Self::ByteArray
            }
            CallbackCompletionPayloadValue::CallbackHandle => Self::Primitive(Primitive::Long),
        }
    }

    fn from_success_out(value: &SuccessOutValue) -> Self {
        match value {
            SuccessOutValue::Scalar { jni_type, .. } => Self::Primitive(Primitive::from(*jni_type)),
            SuccessOutValue::Bytes | SuccessOutValue::Record { .. } => Self::ByteArray,
        }
    }

    fn matches(self, value: &ValueType) -> bool {
        match (self, value) {
            (Self::Primitive(native), ValueType::Primitive(public)) => native == *public,
            (Self::ByteArray | Self::DirectBuffer, ValueType::Record(_))
            | (Self::ByteArray, ValueType::Reference(_)) => true,
            _ => false,
        }
    }

    fn slot_width(self) -> SlotWidth {
        match self {
            Self::Primitive(primitive) => primitive.slot_width(),
            Self::PrimitiveArray(_) | Self::ByteArray | Self::DirectBuffer => SlotWidth::Single,
        }
    }
}

impl fmt::Display for Carrier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive(primitive) => primitive.fmt(formatter),
            Self::PrimitiveArray(primitive) => write!(formatter, "{primitive}[]"),
            Self::ByteArray => formatter.write_str("byte[]"),
            Self::DirectBuffer => formatter.write_str("java.nio.ByteBuffer"),
        }
    }
}

impl JvmParameter for Parameter<Carrier> {
    fn slot_width(&self) -> SlotWidth {
        self.ty().slot_width()
    }
}

#[cfg(test)]
mod tests {
    use crate::target::java::{
        primitive::Primitive,
        render::signature::{ReturnType, ValueType},
    };

    use super::{Carrier, MethodReturn};

    #[test]
    fn preserves_checked_void_as_a_native_only_return_state() {
        let direct = MethodReturn::Void;
        let checked = MethodReturn::CheckedVoid;
        let scalar = MethodReturn::Value(Carrier::Primitive(Primitive::Int));

        assert_eq!(direct.to_string(), "void");
        assert_eq!(checked.to_string(), "void");
        assert_eq!(scalar.to_string(), "int");
        assert!(direct.validate_carrier(&ReturnType::Void).is_ok());
        assert!(checked.validate_carrier(&ReturnType::Void).is_ok());
        assert!(
            checked
                .validate_carrier(&ReturnType::Value(ValueType::Primitive(Primitive::Int)))
                .is_err()
        );
    }
}
