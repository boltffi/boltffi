//! Build pass from C ABI parameter groups to JNI native parameters.
//!
//! The C bridge contract groups low-level ABI pieces before this module sees
//! them. A byte slice is already pointer plus length, a direct vector is already
//! pointer plus count, a continuation is already callback plus context, and a
//! closure is already call, context, and release.
//!
//! This module turns each C group into the Java parameter that should appear in
//! the `Java_*` signature. It is the boundary that prevents method templates
//! from counting adjacent C parameters or guessing that three values are really
//! one closure.

use crate::{
    bridge::{
        c,
        jni::{
            BytesParameter, CallbackParameter, ClosureParameter, ClosureRegistration,
            ContinuationParameter, DirectVectorParameter, NativeParameter, NativeParameterKind,
            RecordParameter, ScalarParameter,
        },
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

impl NativeParameter {
    /// Creates JNI parameters from C ABI parameter groups.
    pub fn from_c_function(
        function: &c::Function,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Vec<Self>> {
        function
            .parameter_groups()
            .iter()
            .map(|group| Self::from_c_group(function, group, callbacks, closures))
            .collect::<Result<Vec<_>>>()
            .map(|parameters| parameters.into_iter().flatten().collect())
    }

    fn from_c_group(
        function: &c::Function,
        group: &c::ParameterGroup,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Option<Self>> {
        match group {
            c::ParameterGroup::Value(index) => {
                Self::from_value_parameter(function.parameter(*index), callbacks).map(Some)
            }
            c::ParameterGroup::ByteSlice(bytes) => BytesParameter::from_c_group(bytes)
                .map(|bytes| Some(Self::new(NativeParameterKind::Bytes(bytes)))),
            c::ParameterGroup::EncodedWriteback(writeback) => {
                BytesParameter::from_c_writeback(writeback)
                    .map(|bytes| Some(Self::new(NativeParameterKind::Bytes(bytes))))
            }
            c::ParameterGroup::DirectVector(vector) => {
                DirectVectorParameter::from_c_group(vector, function)
                    .map(|vector| Some(Self::new(NativeParameterKind::DirectVector(vector))))
            }
            c::ParameterGroup::SuccessOut(_) | c::ParameterGroup::CompletionStatusOut(_) => {
                Ok(None)
            }
            c::ParameterGroup::DirectWriteback(writeback) => {
                RecordParameter::from_c_writeback(writeback, function)
                    .map(|record| Some(Self::new(NativeParameterKind::Record(record))))
            }
            c::ParameterGroup::CallbackCompletion(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback completion parameter group cannot appear on a JNI native method",
            }),
            c::ParameterGroup::Continuation(continuation) => {
                ContinuationParameter::from_c_group(continuation, function).map(|continuation| {
                    Some(Self::new(NativeParameterKind::Continuation(continuation)))
                })
            }
            c::ParameterGroup::Closure(closure) => {
                ClosureParameter::from_c_group(closure, closures)
                    .map(|closure| Some(Self::new(NativeParameterKind::Closure(closure))))
            }
            c::ParameterGroup::ClosureReturn(_) => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "closure return out-pointer on a native method",
            }),
        }
    }

    fn from_value_parameter(parameter: &c::Parameter, callbacks: &[c::Callback]) -> Result<Self> {
        match RecordParameter::from_c_parameter(parameter)? {
            Some(record) => Ok(Self::new(NativeParameterKind::Record(record))),
            None => match CallbackParameter::from_c_parameter(parameter, callbacks)? {
                Some(callback) => Ok(Self::new(NativeParameterKind::Callback(callback))),
                None => ScalarParameter::from_c_parameter(parameter)
                    .map(|scalar| Self::new(NativeParameterKind::Scalar(scalar))),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bridge::c::{Function, Identifier, Parameter, Type};

    use super::*;

    fn point_type() -> Type {
        Type::DirectRecord(Identifier::parse("___Point").expect("record identifier"))
    }

    fn native_parameters_for(point: Type) -> Vec<NativeParameter> {
        let function = Function::new(
            "boltffi_function_demo_distance",
            vec![Parameter::new("point", point).expect("parameter")],
            Type::Float64,
        )
        .expect("function");
        NativeParameter::from_c_function(&function, &[], &[]).expect("jni parameters")
    }

    #[test]
    fn borrowed_direct_record_parameter_remains_record_shaped_for_jni() {
        let params = native_parameters_for(Type::ConstPointer(Box::new(point_type())));

        assert_eq!(params.len(), 1);
        assert!(
            params[0].record().is_some(),
            "borrowed direct records must stay record-shaped instead of falling through to jlong"
        );
    }

    #[test]
    fn borrowed_direct_record_jni_call_argument_addresses_local_copy() {
        let params = native_parameters_for(Type::ConstPointer(Box::new(point_type())));
        let arguments = params[0]
            .c_arguments()
            .expect("record parameter call arguments");

        assert_eq!(arguments.len(), 1);
        assert_eq!(arguments[0].to_string(), "&__boltffi_point_value");
    }

    #[test]
    fn by_value_direct_record_jni_call_argument_stays_local_value() {
        let params = native_parameters_for(point_type());
        let arguments = params[0]
            .c_arguments()
            .expect("record parameter call arguments");

        assert_eq!(arguments.len(), 1);
        assert_eq!(arguments[0].to_string(), "__boltffi_point_value");
    }
}
