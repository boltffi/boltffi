use std::collections::HashSet;

use boltffi_binding::{
    CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl, EnumDecl, ExportedCallable,
    ExportedMethodDecl, FunctionDecl, InitializerDecl, Native, NativeSymbol, RecordDecl,
    StreamDecl,
};

use crate::{
    bridge::{
        c::Identifier as CIdentifier,
        jni::{
            CallbackCompletionInvoker, CallbackCompletionPayload, CallbackCompletionPayloadValue,
            CallbackHandleLifecycle, CallbackHandleMethod, DirectStreamBatchMethod,
            JniBridgeContract, JniType, NativeMethod, NativeParameter, NativeParameterKind,
            SuccessOutValue, SuccessOutWriter,
        },
    },
    core::{Error, Result},
    target::{
        jvm::method::{Parameter as JvmParameter, Parameters as JvmParameters, SlotWidth},
        kotlin::{
            render::type_name::KotlinType,
            syntax::{ArgumentList, Expression, Identifier, TypeName},
        },
    },
};

const JNI_BRIDGE: &str = "jni";

pub struct NativeMethods<'bridge> {
    bridge: &'bridge JniBridgeContract,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunction {
    name: Identifier,
    parameters: JvmParameters<NativeFunctionParameter>,
    returns: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunctionParameter {
    name: Identifier,
    ty: TypeName,
    slot_width: SlotWidth,
}

impl<'bridge> NativeMethods<'bridge> {
    pub fn new(bridge: &'bridge JniBridgeContract) -> Self {
        Self { bridge }
    }

    pub fn function(&self, decl: &FunctionDecl<Native>) -> Result<Vec<NativeFunction>> {
        self.exported(decl.symbol(), decl.callable())
    }

    pub fn record(&self, decl: &RecordDecl<Native>) -> Result<Vec<NativeFunction>> {
        self.associated(decl.initializers(), decl.methods())
    }

    pub fn enumeration(&self, decl: &EnumDecl<Native>) -> Result<Vec<NativeFunction>> {
        self.associated(decl.initializers(), decl.methods())
    }

    pub fn class(&self, decl: &ClassDecl<Native>) -> Result<Vec<NativeFunction>> {
        std::iter::once(decl.release())
            .map(|symbol| self.symbol(symbol).map(|function| vec![function]))
            .chain(std::iter::once(
                self.associated(decl.initializers(), decl.methods()),
            ))
            .collect::<Result<Vec<_>>>()
            .map(|functions| {
                functions
                    .into_iter()
                    .flatten()
                    .collect::<Vec<NativeFunction>>()
            })
    }

    pub fn callback(&self, decl: &CallbackDecl<Native>) -> Result<Vec<NativeFunction>> {
        self.bridge
            .source_callback(decl.id())
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback declaration has no JNI registration",
            })?
            .handle_methods()
            .iter()
            .map(NativeFunction::from_callback_handle_method)
            .collect()
    }

    pub fn stream(&self, decl: &StreamDecl<Native>) -> Result<Vec<NativeFunction>> {
        let protocol = decl.protocol();
        [
            protocol.subscribe(),
            protocol.pop_batch(),
            protocol.wait(),
            protocol.poll(),
            protocol.unsubscribe(),
            protocol.free(),
        ]
        .into_iter()
        .map(|symbol| self.stream_symbol(symbol))
        .collect()
    }

    pub fn constant(&self, decl: &ConstantDecl<Native>) -> Result<Vec<NativeFunction>> {
        match decl.value() {
            ConstantValueDecl::Inline { .. } => Ok(Vec::new()),
            ConstantValueDecl::Accessor { symbol, .. } => {
                self.symbol(symbol).map(|function| vec![function])
            }
            _ => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "unknown constant declaration value",
            }),
        }
    }

    pub fn callback_handle_lifecycle(&self) -> Result<Vec<NativeFunction>> {
        self.bridge
            .callback_handle_lifecycle()
            .map(NativeFunction::from_callback_handle_lifecycle)
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn callback_completions(&self) -> Result<Vec<NativeFunction>> {
        self.bridge
            .callback_completions()
            .iter()
            .map(NativeFunction::from_callback_completion_invoker)
            .collect::<Result<Vec<_>>>()
            .map(|functions| functions.into_iter().flatten().collect())
    }

    pub fn success_out_writers(&self) -> Result<Vec<NativeFunction>> {
        self.bridge
            .success_out_writers()
            .iter()
            .map(NativeFunction::from_success_out_writer)
            .collect()
    }

    fn exported(
        &self,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
    ) -> Result<Vec<NativeFunction>> {
        let mut seen = HashSet::new();
        std::iter::once(symbol)
            .chain(callable.native_symbols())
            .filter(|symbol| seen.insert(symbol.id()))
            .map(|symbol| self.symbol(symbol))
            .collect()
    }

    fn associated(
        &self,
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
    ) -> Result<Vec<NativeFunction>> {
        initializers
            .iter()
            .map(|initializer| self.exported(initializer.symbol(), initializer.callable()))
            .chain(
                methods
                    .iter()
                    .map(|method| self.exported(method.target(), method.callable())),
            )
            .collect::<Result<Vec<_>>>()
            .map(|functions| functions.into_iter().flatten().collect())
    }

    fn symbol(&self, symbol: &NativeSymbol) -> Result<NativeFunction> {
        self.bridge
            .source_method(symbol.id())
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "declaration has no JNI native method",
            })
            .and_then(NativeFunction::from_method)
    }

    fn stream_symbol(&self, symbol: &NativeSymbol) -> Result<NativeFunction> {
        self.bridge
            .source_method(symbol.id())
            .map(NativeFunction::from_method)
            .or_else(|| {
                self.bridge
                    .source_direct_batch(symbol.id())
                    .map(NativeFunction::from_direct_stream_batch)
            })
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "stream declaration has no JNI stream method",
            })?
    }
}

impl NativeFunction {
    pub fn from_method(method: &NativeMethod) -> Result<Self> {
        Self::new(
            Identifier::escape(method.c_function().name())?,
            method
                .parameters()
                .iter()
                .map(NativeFunctionParameter::from_parameter)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            KotlinType::native_return(method.returns())?,
        )
    }

    pub fn from_callback_handle_method(method: &CallbackHandleMethod) -> Result<Self> {
        Self::new(
            Identifier::escape(method.method().as_str())?,
            std::iter::once(NativeFunctionParameter::callback_handle()?)
                .chain(
                    method
                        .parameters()
                        .iter()
                        .map(NativeFunctionParameter::from_parameter)
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .flatten(),
                )
                .collect(),
            KotlinType::callback_handle_return(method)?,
        )
    }

    pub fn from_callback_handle_lifecycle(
        lifecycle: &CallbackHandleLifecycle,
    ) -> Result<Vec<Self>> {
        vec![
            Self::callback_handle_lifecycle_function(lifecycle.clone_method(), TypeName::long()),
            Self::callback_handle_lifecycle_function(lifecycle.release_method(), TypeName::unit()),
        ]
        .into_iter()
        .collect()
    }

    pub fn from_success_out_writer(writer: &SuccessOutWriter) -> Result<Self> {
        Self::new(
            Identifier::escape(writer.method().as_str())?,
            vec![
                NativeFunctionParameter::wide("returnOut", TypeName::long())?,
                NativeFunctionParameter::success_out_value(writer.value())?,
            ],
            TypeName::unit(),
        )
    }

    pub fn from_callback_completion_invoker(
        invoker: &CallbackCompletionInvoker,
    ) -> Result<Vec<Self>> {
        let payload = invoker.payload();
        Ok([
            Some(Self::callback_completion_function(
                invoker.success_method().as_str(),
                payload,
            )?),
            Some(Self::callback_failure_function(
                invoker.failure_method().as_str(),
            )?),
            invoker
                .error_method()
                .map(|method| Self::callback_completion_function(method.as_str(), payload))
                .transpose()?,
        ]
        .into_iter()
        .flatten()
        .collect())
    }

    pub fn from_direct_stream_batch(batch: &DirectStreamBatchMethod) -> Result<Self> {
        Self::new(
            Identifier::escape(batch.c_function().name())?,
            vec![
                NativeFunctionParameter::wide("subscription", TypeName::long())?,
                NativeFunctionParameter::wide("maxCount", TypeName::long())?,
            ],
            TypeName::byte_array(true),
        )
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[NativeFunctionParameter] {
        self.parameters.as_slice()
    }

    pub fn returns(&self) -> &TypeName {
        &self.returns
    }

    fn callback_handle_lifecycle_function(method: &CIdentifier, returns: TypeName) -> Result<Self> {
        Self::new(
            Identifier::escape(method.as_str())?,
            vec![NativeFunctionParameter::callback_handle()?],
            returns,
        )
    }

    fn callback_completion_function(
        name: &str,
        payload: Option<&CallbackCompletionPayload>,
    ) -> Result<Self> {
        Self::new(
            Identifier::escape(name)?,
            [
                NativeFunctionParameter::callback_pointer()?,
                NativeFunctionParameter::callback_context()?,
            ]
            .into_iter()
            .chain(
                payload
                    .map(NativeFunctionParameter::callback_payload)
                    .transpose()?,
            )
            .collect(),
            TypeName::unit(),
        )
    }

    fn callback_failure_function(name: &str) -> Result<Self> {
        Self::new(
            Identifier::escape(name)?,
            vec![
                NativeFunctionParameter::callback_pointer()?,
                NativeFunctionParameter::callback_context()?,
            ],
            TypeName::unit(),
        )
    }

    fn new(
        name: Identifier,
        parameters: Vec<NativeFunctionParameter>,
        returns: TypeName,
    ) -> Result<Self> {
        Ok(Self {
            name,
            parameters: JvmParameters::for_static(parameters)?,
            returns,
        })
    }
}

impl NativeFunctionParameter {
    pub fn from_parameter(parameter: &NativeParameter) -> Result<Vec<Self>> {
        match parameter.kind() {
            NativeParameterKind::Scalar(parameter) => Ok(vec![Self {
                name: Identifier::escape(parameter.name().as_str())?,
                ty: KotlinType::jni(parameter.ty())?,
                slot_width: Self::jni_slot_width(parameter.ty()),
            }]),
            NativeParameterKind::Bytes(parameter) => Ok(vec![
                Self {
                    name: Identifier::escape(parameter.name().as_str())?,
                    ty: TypeName::new("java.nio.ByteBuffer"),
                    slot_width: SlotWidth::Single,
                },
                Self {
                    name: Identifier::escape(parameter.length().as_str())?,
                    ty: TypeName::int(),
                    slot_width: SlotWidth::Single,
                },
            ]),
            NativeParameterKind::Record(parameter) => Ok(vec![Self {
                name: Identifier::escape(parameter.name().as_str())?,
                ty: TypeName::new("java.nio.ByteBuffer"),
                slot_width: SlotWidth::Single,
            }]),
            NativeParameterKind::DirectVector(parameter) => Ok(vec![Self {
                name: Identifier::escape(parameter.name().as_str())?,
                ty: KotlinType::jni_array(parameter.jni_type())?,
                slot_width: SlotWidth::Single,
            }]),
            NativeParameterKind::Callback(parameter) => Ok(vec![Self {
                name: Identifier::escape(parameter.name().as_str())?,
                ty: TypeName::long(),
                slot_width: SlotWidth::Double,
            }]),
            NativeParameterKind::Closure(parameter) => Ok(vec![Self {
                name: Identifier::escape(parameter.name().as_str())?,
                ty: TypeName::long(),
                slot_width: SlotWidth::Double,
            }]),
            NativeParameterKind::Continuation(parameter) => Ok(vec![Self {
                name: Identifier::escape(parameter.name().as_str())?,
                ty: TypeName::long(),
                slot_width: SlotWidth::Double,
            }]),
        }
    }

    pub fn callback_handle() -> Result<Self> {
        Self::wide("handle", TypeName::long())
    }

    pub fn callback_pointer() -> Result<Self> {
        Self::wide("callback", TypeName::long())
    }

    pub fn callback_context() -> Result<Self> {
        Self::wide("context", TypeName::long())
    }

    pub fn callback_payload(payload: &CallbackCompletionPayload) -> Result<Self> {
        let (ty, slot_width) = match payload.value() {
            CallbackCompletionPayloadValue::Scalar(jni_type) => {
                (KotlinType::jni(jni_type)?, Self::jni_slot_width(jni_type))
            }
            CallbackCompletionPayloadValue::Bytes | CallbackCompletionPayloadValue::Record => {
                (TypeName::byte_array(false), SlotWidth::Single)
            }
            CallbackCompletionPayloadValue::CallbackHandle => (TypeName::long(), SlotWidth::Double),
        };
        Ok(Self {
            name: Identifier::parse("result")?,
            ty,
            slot_width,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn success_out_value(value: &SuccessOutValue) -> Result<Self> {
        let (ty, slot_width) = match value {
            SuccessOutValue::Scalar { jni_type, .. } => {
                (KotlinType::jni(*jni_type)?, Self::jni_slot_width(*jni_type))
            }
            SuccessOutValue::Bytes | SuccessOutValue::Record { .. } => {
                (TypeName::byte_array(false), SlotWidth::Single)
            }
        };
        Ok(Self {
            name: Identifier::parse("value")?,
            ty,
            slot_width,
        })
    }

    fn wide(name: &str, ty: TypeName) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(name)?,
            ty,
            slot_width: SlotWidth::Double,
        })
    }

    fn jni_slot_width(ty: JniType) -> SlotWidth {
        match ty {
            JniType::Long | JniType::Double => SlotWidth::Double,
            JniType::Boolean | JniType::Byte | JniType::Short | JniType::Int | JniType::Float => {
                SlotWidth::Single
            }
        }
    }
}

impl JvmParameter for NativeFunctionParameter {
    fn slot_width(&self) -> SlotWidth {
        self.slot_width
    }
}

pub struct NativeCall {
    function: Identifier,
    arguments: Vec<Expression>,
}

impl NativeCall {
    pub fn new(function: Identifier, arguments: Vec<Expression>) -> Self {
        Self {
            function,
            arguments,
        }
    }

    pub fn expression(&self) -> Expression {
        Expression::call(
            "Native",
            self.function.clone(),
            self.arguments.iter().cloned().collect::<ArgumentList>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeFunction, NativeFunctionParameter};
    use crate::target::kotlin::syntax::{Identifier, TypeName};

    #[test]
    fn rejects_native_functions_over_the_jvm_slot_limit() {
        let parameters = std::iter::repeat_with(NativeFunctionParameter::callback_handle)
            .take(128)
            .collect::<crate::Result<Vec<_>>>()
            .unwrap();

        assert!(
            NativeFunction::new(
                Identifier::parse("overflow").unwrap(),
                parameters,
                TypeName::unit(),
            )
            .is_err()
        );
    }
}
